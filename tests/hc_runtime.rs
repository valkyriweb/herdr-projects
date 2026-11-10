//! hc transport and the real board lifecycle: launch, submission recovery, missed turn end, review and cleanup.
use crate::common::{self, *};
use sw::{herdr::{hc, Herdr}, json::*};
fn world_hc() -> World { world_with(Opts { repo: true, settings: json!({ "agents.runtime": "hc", "worker.agent": "claude", "reviewer.agent": "codex" }), ..Default::default() }) }

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
    core.unread.set(1);
    w.patch(|s| s["swallow"] = json!(1));
    w.script(json!({ "worker": [step(1)] }));
    let id = w.add("Starting slowly"); delegate(&id);
    let t = settle(&id);
    assert!(core.text().contains("hc keys sw-proj-1 enter"), "the lost enter is pressed again: {}", core.text());
    assert_eq!(t.s("stage"), "approval", "{} {}", t.s("msg"), t.at("failure"));
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
