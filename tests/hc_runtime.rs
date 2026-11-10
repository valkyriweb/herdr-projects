//! hc transport and the real board lifecycle: launch, submission recovery, missed turn end, review and cleanup.
use crate::common::{self, *};
use sw::{herdr::{hc, Herdr}, json::*};
fn world_hc() -> World { world_with(Opts { repo: true, settings: json!({ "agents.runtime": "hc", "worker.agent": "claude", "reviewer.agent": "codex" }), ..Default::default() }) }

fn in_hc(pane: &str) {
    unsafe {
        std::env::remove_var("HERDR_PANE_ID");
        std::env::set_var("HERDR_PROJECTS_PANE", pane);
    }
}
fn report(id: &str, state: &str) -> sw::err::Res<(Value, String)> {
    let argv = vec!["report".into(), id.into(), "--state".into(), state.into()];
    sw::cli::tasks::command("report", &sw::cli::flags::parse(&argv))
}

#[test]
fn hc_worker_cannot_mark_its_own_task_done() {
    let w = world_hc(); let _core = common::hc::Core::install(&w);
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("Worker identity"); delegate(&id); let t = settle(&id);
    in_hc(t.at("link").s("pane"));
    let result = report(&id, "done");
    unsafe { std::env::remove_var("HERDR_PROJECTS_PANE"); }
    let e = result.unwrap_err().msg;
    assert!(e.contains(&format!("{id} is your own task, so you cannot mark it done")), "{e}");
    assert!(e.contains("Your task finishes when you end your turn; the board then checks, reviews and merges it"), "{e}");
    assert_eq!(task(&id).s("stage"), "working");
    in_hc(t.at("link").s("pane"));
    let args = ["report", "--msg", "Still working"].map(String::from);
    sw::cli::tasks::command("report", &sw::cli::flags::parse(&args)).unwrap();
    let other = w.add("Not my task");
    sw::store::tasks::set(&id, json!({ "autopilot": { "coordinator": "test" } })).unwrap();
    let refused = report(&other, "done").unwrap_err().msg;
    unsafe { std::env::remove_var("HERDR_PROJECTS_PANE"); }
    assert!(refused.contains("may read the board and `report` on its own task, nothing else"), "{refused}");
}

#[test]
fn hc_reviewer_is_recognised_and_cannot_finish_the_task() {
    let w = world_hc(); let _core = common::hc::Core::install(&w);
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("Reviewer identity"); delegate(&id); let t = settle(&id);
    let api = sw::flow::task::h(&t);
    let made = sw::flow::tabs::open_reviewer_tab(&api, &t, 1, "", t.s("worktree"), &[], None).unwrap();
    let reviewer = json!({ "pane": made.pane, "tab": made.tab, "terminal": made.terminal, "agent": "identity-r", "kind": "codex" });
    sw::store::tasks::update(&id, |t| { t.obj_mut("link").put("reviewer", reviewer.clone()); Ok(sw::store::tasks::Change::Write) }).unwrap();
    api.agent_start("identity-r", "codex", reviewer.s("pane"), &[], 30000).unwrap();
    in_hc(reviewer.s("pane"));
    let caller = sw::cli::caller();
    let result = report(&id, "done");
    unsafe { std::env::remove_var("HERDR_PROJECTS_PANE"); }
    assert_eq!(caller.map(|(t, role)| (t.t("id"), role)), Some((id.clone(), "reviewer")));
    assert!(result.unwrap_err().msg.contains("is the task you review, so you cannot mark it done"));
}

#[test]
fn hc_task_can_still_be_finished_by_a_non_task_caller() {
    let w = world_hc(); let _core = common::hc::Core::install(&w);
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("Human identity"); delegate(&id); settle(&id);
    unsafe { std::env::remove_var("HERDR_PROJECTS_PANE"); std::env::remove_var("HERDR_PANE_ID"); }
    assert!(sw::cli::caller().is_none());
    report(&id, "done").unwrap();
    assert_eq!(task(&id).s("stage"), "done");
}

#[test]
fn classic_caller_identity_takes_precedence_over_hc() {
    let w = world_hc();
    in_hc("hc-other");
    unsafe { std::env::set_var("HERDR_PANE_ID", "classic-pane"); }
    assert_eq!(sw::herdr::cli::caller_place(), Some(("classic-pane".into(), w.socket.clone())));
    unsafe { std::env::remove_var("HERDR_SOCKET_PATH"); }
    assert!(sw::herdr::cli::caller_place().is_none(), "a classic pane without a socket must not become an hc caller");
    unsafe { std::env::set_var("HERDR_PANE_ID", ""); std::env::set_var("HERDR_SOCKET_PATH", &w.socket); }
    assert!(sw::herdr::cli::caller_place().is_none(), "an explicitly empty classic pane must stay a non-task caller");
    unsafe { std::env::remove_var("HERDR_PROJECTS_PANE"); std::env::remove_var("HERDR_PANE_ID"); std::env::set_var("HERDR_SOCKET_PATH", &w.socket); }
}

#[test]
fn hc_turn_end_fires_the_review_and_keeps_durable_task_mapping() {
    let w = world_hc(); let core = common::hc::Core::install(&w);
    w.script(json!({ "worker": [step(1)] }));
    let id = w.add("Run in the app"); delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "approval", "{} {}", t.s("msg"), t.at("failure"));
    assert_eq!(t.at("link").s("socket"), hc::SOCKET);
    let pane = t.at("link").s("pane");
    let place = hc::places::get(pane).unwrap();
    assert!(place.s("target").starts_with("sw-proj-"));
    // A fresh API instance finds the same agent through the saved terminal name.
    let api = Herdr::new(Some(hc::SOCKET.into()));
    assert_eq!(api.agent_get(pane).unwrap().unwrap().s("name"), t.at("link").s("agent"));
    assert!(core.text().contains("--label herdr.space=sp-project"));
    assert!(core.text().contains(&format!("--label herdr-projects.task={id}")));
    assert!(core.text().contains("hc run --json --bg --no-parent --keep"));
    assert!(core.text().contains(&format!("HERDR_PROJECTS_PANE={pane}")));
    assert!(!core.text().contains("hc keys sw-proj-1 enter"), "a fast completed turn must not get a second submit");
    assert!(core.text().contains("--name sw-proj-1-r"), "{}", core.text());
    assert!(core.text().contains("hc rm sw-proj-1-r"), "reviewer closes after its verdict");
    assert!(!calls(&w).iter().any(|c| c.starts_with("worktree create")), "the hc backend creates worktrees through git, not classic herdr");
    let ws = t.at("link").s("workspace");
    api.workspace_close(ws).unwrap();
    assert!(hc::places::get(pane).is_err());
    assert!(core.text().contains("hc keep sw-proj-1 --off"));
    assert!(core.text().contains("hc kill sw-proj-1"));
    assert!(core.text().contains("hc rm sw-proj-1"));
    // Closing a logical task workspace does not close the project's shared app space.
    assert!(!core.text().contains("space.close"));
}

#[test]
fn hc_submits_a_boot_time_paste_and_stop_interrupts_the_agent() {
    let w = world_hc(); let core = common::hc::Core::install(&w);
    w.patch(|s| s["swallow"] = json!(1));
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("Still working"); delegate(&id);
    let t = settle(&id);
    assert_eq!(t.s("stage"), "working", "{}", t.at("failure"));
    let api = sw::flow::task::h(&t);
    let pane = t.at("link").s("pane");
    sw::actions::talk::stop(&id).unwrap();
    assert_eq!(task(&id).s("stage"), "stopped");
    assert!(core.text().contains("hc interrupt sw-proj-1"));
    assert!(core.text().contains("hc keys sw-proj-1 enter"), "a swallowed boot-time paste gets exactly its missing submit");
    sw::clock::set(Some(sw::clock::now()));
    core.kill_lag.set(1);
    assert!(api.pane_quit(pane, 0));
    api.workspace_close(t.at("link").s("workspace")).unwrap();
    assert!(api.agent_get(pane).unwrap().is_none());
}

#[test]
fn hc_first_status_of_a_starting_program_is_not_taken_for_the_prompt() {
    // Seen on pi 1.5.1: the board typed the brief while hc had no status for the new program yet; the enter was lost,
    // and the program's first resting status moved status_ms, which read as "it took the text". It sat with the brief
    // in its box until someone pressed enter.
    let w = world_hc(); let core = common::hc::Core::install(&w);
    unsafe { std::env::set_var("HERDR_PROJECTS_READY_MS", "0"); std::env::set_var("HERDR_PROJECTS_SUBMIT_MS", "0"); }
    core.unread.set(1);
    w.patch(|s| s["swallow"] = json!(1));
    w.script(json!({ "worker": [step(1)] }));
    let id = w.add("Starting slowly"); delegate(&id);
    settle(&id);
    // The board's housekeeping looks again at what it typed; the first resting status must not end that look.
    for _ in 0..10 {
        sw::housekeeping::housekeeping(&w.socket, sw::housekeeping::Force::Yes); settle(&id);
        if task(&id).s("stage") != "working" { break; }
    }
    let t = task(&id);
    assert_eq!(core.text().matches("hc keys sw-proj-1 enter").count(), 1, "the lost enter is pressed again, once: {}", core.text());
    let a = agent_of(&w, t.at("link").s("pane"));
    assert_eq!(t.s("stage"), "approval", "msg={} failure={} sent={} unconfirmed={} agent={} calls:\n{}", t.s("msg"), t.at("failure"), t.at("sent"), t.at("unconfirmed"), a, core.text());
}

#[test]
fn hc_starts_without_the_app_and_classic_links_do_not_switch_runtime() {
    let w = world_hc(); let core = common::hc::Core::install(&w);
    // A missing folder cannot be canonicalized, but a trailing slash must not create a second app space.
    let missing = w.root.join("missing-project").display().to_string();
    *core.spaces.borrow_mut() = json!([{ "id": "sp-trailing", "name": "Custom name", "folder": format!("{missing}/") }]);
    let opened = sw::herdr::hc::places::create(&missing, &missing, "Scratch", vec![], Value::Null).unwrap();
    let ws = opened.at("workspace").s("workspace_id");
    assert_eq!(sw::herdr::hc::places::get(ws).unwrap().s("space"), "sp-trailing");
    assert!(!core.text().contains("space.new"));
    sw::herdr::hc::places::close(ws).unwrap();
    *core.app.borrow_mut() = false;
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("No app"); delegate(&id); let t = settle(&id);
    assert_eq!(t.s("stage"), "working", "{}", t.at("failure"));
    assert!(!core.text().contains("--label herdr.space="));
    assert!(read(&sw::store::files::data_dir().join("hc/runtime.log")).contains("without an app space"));
    let before = core.calls.borrow().len();
    Herdr::new(Some(w.socket.clone())).workspaces().unwrap();
    assert_eq!(core.calls.borrow().len(), before, "explicit classic socket stays classic");
}
