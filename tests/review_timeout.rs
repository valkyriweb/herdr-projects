//! `review.timeout`: a review that runs past it fails and says the limit, `none` waits on, a task's own value wins over
//! yours, and the repo's own file cannot set it. What a timed-out reviewer had so far is kept as a timeout verdict, and
//! `retry` resumes the review with it: no second check, and the next reviewer is told what the last one had.
use crate::common;
use common::*;
use serde_json::json;
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
    assert!(t.at("failure").s("detail").contains("It had produced nothing the board could read."), "{}", t.at("failure"));
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
