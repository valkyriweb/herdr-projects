//! Mixed-runtime safety and numeric hc turn counters through the board's hook boundary.
use crate::common::{self, *};
use sw::{herdr::{cli::current_socket, hc}, json::*, store::settings::{change::set_value, lookup::Scope}};
fn runtime(value: &str) { set_value("agents.runtime", &json!(value), &Scope::you()).unwrap(); }

#[test]
fn changing_runtime_keeps_classic_and_hc_tasks_actionable_in_one_board() {
    let w = world_with(Opts { repo: true, settings: json!({ "worker.agent": "claude" }), ..Default::default() });
    let core = common::hc::Core::install(&w);
    w.script(json!({ "worker": [{ "status": "working" }, { "status": "working" }, { "status": "working" }] }));
    let classic = w.add("Classic stays classic"); delegate(&classic); let c = settle(&classic);
    assert_eq!(c.at("link").s("socket"), w.socket);
    runtime("hc");
    assert_eq!(current_socket(), w.socket, "classic event context must not follow the launch setting");
    sw::actions::talk::tell(&classic, "Keep going", false).unwrap();
    sw::hook::on_pane(&current_socket(), c.at("link").s("pane"));
    assert_eq!(task(&classic).at("last").s("status"), "working");
    sw::actions::talk::stop(&classic).unwrap();
    assert_eq!(task(&classic).s("stage"), "stopped");
    delegate(&classic); settle(&classic);
    assert_eq!(task(&classic).at("link").s("socket"), w.socket);
    let hc_task = w.add("New runtime"); delegate(&hc_task); let h = settle(&hc_task);
    assert_eq!(h.at("link").s("socket"), hc::SOCKET);
    assert!(h.b("startedOnce"), "hc delegation did not start: {h:?}\n{}", core.text());
    runtime("classic");
    sw::actions::talk::tell(&hc_task, "Keep going on hc", false).unwrap();
    assert!(!row(&hc_task).at("live").b("away"));
    sw::flow::live::assert_live(&h).unwrap();
    sw::actions::talk::stop(&hc_task).unwrap();
    assert_eq!(task(&hc_task).s("stage"), "stopped");
    assert!(core.text().contains("hc interrupt"));
}

#[test]
fn numeric_hc_turn_increase_from_working_to_done_arms_the_finish_pipeline() {
    let w = world_with(Opts { repo: true, settings: json!({ "agents.runtime": "hc", "worker.agent": "claude" }), ..Default::default() });
    let core = common::hc::Core::install(&w);
    w.script(json!({ "worker": [{ "status": "working" }] }));
    let id = w.add("Numeric hc turn"); delegate(&id); let t = settle(&id);
    let pane = t.at("link").s("pane");
    let api = sw::flow::task::h(&t);
    *core.get_override.borrow_mut() = Some(json!({ "address": "t7", "state": "live", "status": { "app": "claude-code", "state": "working" }, "status_ms": 1_790_000_000_100i64, "turn": 674 }));
    let working = api.agent_get(pane).unwrap().unwrap();
    assert_eq!(working.i("state_change_seq"), 1_790_000_000_100);
    assert_eq!(working.s("agent_status"), "working", "app aliases must not look like an exit");
    sw::hook::on_pane(hc::SOCKET, pane);
    *core.get_override.borrow_mut() = Some(json!({ "address": "t7", "state": "live", "status": { "app": "claude-code", "state": "done" }, "status_ms": 1_790_000_000_200i64, "turn": 675 }));
    sw::hook::on_pane(hc::SOCKET, pane);
    let t = task(&id);
    assert_eq!(t.at("last").s("status"), "done");
    assert_eq!(t.at("last").s("comp"), "t7:675");
    assert!(sw::runner::JOBS.with(|j| j.borrow().iter().any(|(job, task, _)| job == "finish" && task == &id)));
    *core.get_override.borrow_mut() = Some(json!({ "address": "t7", "state": "finished", "status": null, "status_ms": null, "turn": 675 }));
    assert!(api.agent_get(pane).unwrap().is_none());
}
