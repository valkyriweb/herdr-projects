//! `review.timeout`: a review that runs past it fails and says the limit, `none` waits on, a task's own value wins over
//! yours, and the repo's own file cannot set it. A review that ends without a verdict quotes the reviewer's last screen.
//! What a timed-out reviewer had so far is kept as a timeout verdict, and `retry` resumes the review with it: no second
//! check, and the next reviewer is told what the last one had. Its whole folder survives the retry, its text is capped with
//! credentials hidden, it never stands as a passing review, and a retry does not run a shorter check twice.
use crate::common;
use common::*;
use serde_json::{json, Value};
use sw::json::*;
use sw::store::settings::lookup::setting_text;

#[test]
fn a_review_past_its_limit_fails_and_says_the_limit_the_tasks_own_value_wins_over_yours() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "review.timeout": "20m" }), ..Default::default() });
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }], "reviewer": [{ "status": "working" }, { "status": "working" }] }));
    let id = w.add("Slow review");
    delegate_with(&id, json!({ "review.timeout": "1s" }));
    let t = settle(&id);
    assert_eq!(t.s("stage"), "failed", "{}", t.at("failure"));
    assert!(t.at("failure").s("detail").contains("review timed out after 1s (review.timeout)"), "{}", t.at("failure"));
}

/// The prompts every reviewer was given, in the order they were started.
fn reviewer_prompts(w: &World) -> Vec<String> {
    w.state().arr("prompted").iter().filter(|p| regex::Regex::new(r"-r(-\d+)?$").unwrap().is_match(p.s("name"))).map(|p| p.t("text")).collect()
}

#[test]
fn what_a_timed_out_reviewer_had_is_kept_and_retry_resumes_the_review_without_a_second_check() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "review.timeout": "1s", "check.command": "echo ran >> \"$HERDR_PROJECTS_REPO/.git/checks.log\"" }), ..Default::default() });
    let pass = json!({ "verdict": { "meets": true, "findings": [], "text": "F1 was right and is fixed; nothing else." } });
    // The first reviewer writes notes in the review folder and shows a finding on its screen, then never answers.
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }],
        "reviewer": [{ "status": "working", "evidence": { "notes.md": "F1 major: f.txt lacks a newline check\n" }, "screen": "Reading f.txt…\nFinding so far: F1 major, f.txt lacks a newline check\nStill checking the tests" }, pass] }));
    let id = w.add("Slow review with notes");
    delegate(&id);
    let t = settle(&id);
    // Before this change the task kept nothing: `review` was null and the failure only said "timed out".
    assert_eq!(t.s("stage"), "failed", "{}", t.at("failure"));
    assert!(t.at("failure").s("detail").contains("review timed out after 1s (review.timeout)"), "{}", t.at("failure"));
    assert_eq!(t.at("failure").s("title"), "Review timed out");
    assert_eq!(t.at("failure").at("resume"), &json!(true));
    assert!(t.at("failure").s("detail").contains("F1 major, f.txt lacks a newline check"), "{}", t.at("failure"));
    assert!(t.s("msg").starts_with("Review timed out after 1s with partial findings kept"), "{}", t.s("msg"));
    let rv = t.at("review");
    assert_eq!(rv.at("timeout").s("stage"), "review");
    assert_eq!(rv.at("timeout").s("reason"), "timeout");
    assert_eq!(rv.at("timeout").s("limit"), "1s");
    assert!(rv.at("timeout").i("elapsed") >= 1000, "{}", rv.at("timeout"));
    assert_eq!(rv.at("timeout").i("bytes"), rv.s("partial").len() as i64);
    assert!(rv.s("partial").contains("## notes.md\nF1 major: f.txt lacks a newline check"), "{}", rv.s("partial"));
    assert!(rv.s("partial").contains("## Its screen when it was stopped\nReading f.txt…"), "{}", rv.s("partial"));
    assert_eq!(rv.s("error"), "review timed out after 1s (review.timeout)");
    // The same on the task's history, and in `details`.
    let hist = sw::store::reviews::review_history(&t);
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].at("timeout").s("reason"), "timeout");
    assert_eq!(hist[0].at("partial"), rv.at("partial"));
    let d = sw::actions::finish_line::details(&id).unwrap();
    assert_eq!(d.at("review").at("timeout").s("reason"), "timeout");
    assert_eq!(d.at("reviews")[0].at("partial"), rv.at("partial"));
    let log = w.root.join(".git").join("checks.log");
    assert_eq!(read(&log).lines().count(), 1, "the check ran once before the review");
    // Retry: the review starts again on the same commit, the check is not run again, and the next reviewer is told
    // what the first one had.
    let said = sw::actions::finish_line::retry(&id).unwrap();
    assert!(said.msg.ends_with(": resuming the review"), "{}", said.msg);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{} {}", t.s("msg"), t.at("failure"));
    assert_eq!(read(&log).lines().count(), 1, "the check was run again on the same commit");
    assert!(t.at("review").at("pass") == &json!(true), "{}", t.at("review"));
    let hist = sw::store::reviews::review_history(&t);
    assert_eq!(hist.len(), 2, "{hist:?}");
    assert_eq!(hist[0].at("timeout").s("reason"), "timeout");
    assert_eq!(hist[1].at("pass"), &json!(true));
    let prompts = reviewer_prompts(&w);
    let second = prompts.last().unwrap();
    assert!(second.contains("A reviewer before you ran out of time on this same commit (its limit was 1s"), "{second}");
    assert!(second.contains("F1 major: f.txt lacks a newline check"), "{second}");
    assert!(!prompts[0].contains("ran out of time"), "the first reviewer heard of no earlier one");
}

#[test]
fn a_timed_out_reviewer_that_left_nothing_says_so() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "review.timeout": "1s" }), ..Default::default() });
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }], "reviewer": [{ "status": "working" }, { "status": "working" }] }));
    let id = w.add("Silent slow review");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "failed");
    assert_eq!(t.at("review").at("timeout").i("bytes"), 0);
    assert_eq!(t.s("msg"), "Review timed out after 1s. Retry resumes the review");
    assert!(t.at("failure").s("next").contains("It had produced nothing the board could read."), "{}", t.at("failure"));
    // With nothing left behind the next reviewer starts fresh.
    sw::actions::finish_line::retry(&id).unwrap();
    settle(&id);
    assert!(!reviewer_prompts(&w).last().unwrap().contains("ran out of time"));
}

#[test]
fn none_waits_on_for_a_slow_reviewer() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "review.timeout": "1s" }), ..Default::default() });
    let pass = json!({ "verdict": { "meets": true, "findings": [], "text": "Fine." } });
    // The reviewer works on for many looks of the board (one every 20ms), well past your 1s, then answers.
    w.script(json!({ "worker": [{ "files": { "f.txt": "v1\n" }, "commit": "v1", "note": "Did v1." }], "reviewer": [{ "status": "working", "then": pass, "after": 120 }] }));
    let id = w.add("Slow but fine");
    delegate_with(&id, json!({ "review.timeout": "none" }));
    let began = std::time::Instant::now();
    let t = settle(&id);
    assert!(began.elapsed().as_millis() > 1000, "the reviewer answered within the 1s it would have had");
    assert_eq!(t.s("stage"), "approval", "{}", t.at("failure"));
}

#[test]
fn the_repos_own_file_cannot_set_it() {
    let w = world_with(Opts { repo: true, repo_settings: Some(json!({ "review.timeout": "1s" })), ..Default::default() });
    let id = w.add("Any");
    assert_eq!(setting_text("review.timeout", Some(&task(&id))), "20m");
}

#[test]
fn a_review_that_ends_without_a_verdict_quotes_the_reviewers_last_screen() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0, "review.timeout": "1s" }), ..Default::default() });
    // A dialog herdr does not report as one: the reviewer stays "working" on a screen that does not move.
    let earlier: String = (1..=9).map(|n| format!("earlier line {n}\n")).collect();
    let dialog = format!("{earlier}\n╭──────────────────────╮\n│ Allow this command?  │\n│ ❯ 1. Yes             │\n│   2. No              │\n╰──────────────────────╯\n");
    w.script(json!({ "worker": [step(1)], "reviewer": [{ "status": "working", "screen": dialog }] }));
    let id = w.add("Stuck review");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "failed", "{}", t.at("failure"));
    let detail = t.at("failure").s("detail");
    assert!(detail.starts_with("Review failed: review timed out after 1s (review.timeout)\n\nThe reviewer's screen when the review ended:\n"), "{detail}");
    // Its last 8 lines, its words without the dialog's frame.
    assert!(detail.ends_with("earlier line 5\nearlier line 6\nearlier line 7\nearlier line 8\nearlier line 9\nAllow this command?\n❯ 1. Yes\n  2. No"), "{detail}");
    assert!(!detail.contains("earlier line 4"), "{detail}");
    assert!(!t.s("msg").contains("1. Yes"), "the row's line stays one short line: {}", t.s("msg"));
}

#[test]
fn a_reviewer_whose_terminal_closes_says_so() {
    let w = world_with(Opts { repo: true, settings: json!({ "review.rounds": 0 }), ..Default::default() });
    // Its terminal closes while it reviews, in the first try and in the one the board starts once more by itself.
    let closes = json!({ "status": "working", "then": { "closed": true }, "after": 2 });
    w.script(json!({ "worker": [step(1)], "reviewer": [closes.clone(), closes] }));
    let id = w.add("Closed review");
    delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "failed", "{}", t.at("failure"));
    assert_eq!(t.at("failure").s("detail"), "Review failed: the reviewer's terminal was closed", "{}", t.at("failure"));
}

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
