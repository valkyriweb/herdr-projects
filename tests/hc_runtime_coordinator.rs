//! Coordinator launches select hc once while board layout and saved coordinator identity stay classic-session scoped.
use crate::common::{self, *};
use sw::{json::*, store::settings::{change::set_value, lookup::Scope}};
#[test]
fn a_new_coordinator_runs_on_hc_and_keeps_it_after_the_setting_changes() {
    let w = crate::coordinator::coord_world();
    let core = common::hc::Core::install(&w);
    set_value("agents.runtime", &json!("hc"), &Scope::you()).unwrap();
    w.script(json!({ "coordinator": [{ "said": "Ready." }, { "said": "Still ready." }] }));
    sw::coordinator::requests::send(&w.root, "Hello", false, "", w.socket.as_str()).unwrap();
    sw::runner::drain_jobs(&|job, id, extra| { sw::jobs::run_job(job, id, extra); flush(); });
    let c = sw::coordinator::state::get(&w.socket).unwrap();
    assert!(c.s("pane").starts_with("hc-"), "{c:?}");
    assert!(core.text().contains("--name sw-coordinator"));
    assert_eq!(sw::herdr::cli::current_socket(), w.socket);
    set_value("agents.runtime", &json!("classic"), &Scope::you()).unwrap();
    assert!(sw::coordinator::state::api(w.socket.as_str()).agent_get(c.s("pane")).unwrap().is_some());
    sw::coordinator::requests::send(&w.root, "Again", false, "", w.socket.as_str()).unwrap();
    sw::runner::drain_jobs(&|job, id, extra| { sw::jobs::run_job(job, id, extra); flush(); });
    assert_eq!(sw::coordinator::state::get(&w.socket).unwrap().s("pane"), c.s("pane"));
    assert!(core.text().contains("hc send sw-coordinator Again"));
}
