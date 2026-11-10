//! Launch and observe hc agents without shell PATH assumptions or classic process inspection.
use super::{error, json as run_json, places, raw};
use crate::{herdr::cli::{args, HRes}, json::*, store::tasks::{list, ListOpts}};
use std::path::Path;
pub(super) fn executable(program: &str) -> HRes<String> {
    if program.contains('/') { return Ok(program.into()); }
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).map(|p| p.join(program)).find(|p| {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    }).map(|p| p.display().to_string()).ok_or_else(|| error("program_missing", format!("{program} was not found on the board's PATH")))
}
pub fn terminal(p: &Value) -> HRes<Option<Value>> {
    if !p.b("started") { return Ok(None); }
    match run_json("hc", &args(&["get", p.s("target")])) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.message.contains("not found") || e.message.contains("no terminal") => Ok(None),
        Err(e) => Err(e),
    }
}
pub fn agent(id: &str) -> HRes<Option<Value>> {
    let p = places::get(id)?;
    if !p.b("running") { return Ok(None); }
    let Some(t) = terminal(&p)? else { return Ok(None) };
    let state = t.at("status").s("state");
    // Kept terminals recover to a shell. The old agent is gone even though hc still has a terminal.
    let command = t.arr("argv").first().map(text).unwrap_or_default();
    let shell = Path::new(&command).file_name().and_then(|s| s.to_str()).map(|s| ["sh", "bash", "zsh", "fish", "nu"].contains(&s)).unwrap_or(false);
    if t.s("state") == "finished" || state == "exited" || (t.at("status").is_null() && shell) { return Ok(None); }
    let mut a = places::pane(&p);
    a["name"] = p.at("name").clone(); a["agent"] = p.at("kind").clone();
    a["agent_status"] = json!(if ["working", "idle", "done", "blocked"].contains(&state) { state } else { "unknown" });
    a["state_change_seq"] = t.at("status_ms").clone();
    if t.i("turn") > 0 { a["completion_seq"] = json!(format!("{}:{}", t.s("address"), t.i("turn"))); }
    if t.at("foreground").b("cwd") { a["foreground_cwd"] = t.at("foreground").at("cwd").clone(); }
    Ok(Some(a))
}
pub fn start(id: &str, name: &str, kind: &str, extra: &[String]) -> HRes<Value> {
    let mut p = places::get(id)?;
    if p.b("started") { remove(id)?; }
    let program = executable(crate::agents::kinds::program(kind))?;
    let task = list(ListOpts { attempts: true, ..Default::default() }).into_iter().find(|t| t.at("link").s("pane") == id || t.at("link").at("reviewer").s("pane") == id);
    let target = name.to_string();
    p["target"] = json!(target);
    let mut a = args(&["run", "--json", "--bg", "--no-parent", "--keep", "--cwd", p.s("cwd"), "--name", &target]);
    if p.b("space") { a.extend(args(&["--label", &format!("herdr.space={}", p.s("space"))])); }
    // One stable app tab per board place. The app creates the tab for this label; no focus changes.
    a.extend(args(&["--label", &format!("herdr.tab={id}"), "--label", &format!("where={} / {name}", Path::new(p.s("root")).file_name().unwrap_or_default().to_string_lossy())]));
    if let Some(t) = task { a.extend(args(&["--label", &format!("herdr-projects.task={}", t.s("id"))])); }
    a.extend(args(&["--", "/usr/bin/env"])); a.extend(strings(p.at("env"))); a.push(program); a.extend_from_slice(extra);
    // Save the launch intent first so a board crash cannot orphan a successfully started terminal.
    p["started"] = json!(true); p["running"] = json!(true); p["kind"] = json!(kind); p["name"] = json!(name);
    places::save(&p)?;
    run_json("hc", &a)?;
    Ok(agent(id)?.unwrap_or_else(|| places::pane(&p)))
}
pub fn kill(id: &str) -> HRes<()> {
    let mut p = places::get(id)?;
    if terminal(&p)?.map(|t| t.s("state") != "finished").unwrap_or(false) {
        raw("hc", &args(&["keep", p.s("target"), "--off"]))?;
        raw("hc", &args(&["kill", p.s("target")]))?;
        let mut finished = false;
        for _ in 0..100 {
            if terminal(&p)?.map(|t| t.s("state") == "finished").unwrap_or(true) { finished = true; break; }
            crate::clock::pause(100);
        }
        if !finished { return Err(error("timeout", "hc agent has not exited after kill; retry cleanup")); }
    }
    p["running"] = json!(false); p["kind"] = Value::Null;
    places::save(&p)
}
pub fn remove(id: &str) -> HRes<()> {
    let p = places::get(id)?;
    if terminal(&p)?.is_none() { return Ok(()); }
    kill(id)?;
    match raw("hc", &args(&["rm", p.s("target")])) {
        Ok(_) => Ok(()),
        Err(e) if e.message.contains("not found") || e.message.contains("no terminal") => Ok(()),
        Err(e) => Err(e),
    }
}
pub fn prompt(id: &str, text: &str) -> HRes<Value> {
    let p = places::get(id)?;
    let before = terminal(&p)?.ok_or_else(|| error("agent_not_found", "The hc agent is gone"))?;
    if before.at("status").s("state") == "blocked" { return Err(error("agent_blocked", "The hc agent is blocked")); }
    raw("hc", &args(&["send", p.s("target"), text]))?;
    // Only retry submit when the same resting turn is still pending. A fast completed turn or
    // a working agent must not receive a second enter (which can submit a queued message).
    if let Some(after) = terminal(&p)? {
        if ["idle", "done"].contains(&after.at("status").s("state")) && after.at("status_ms") == before.at("status_ms") && after.at("turn") == before.at("turn") {
            raw("hc", &args(&["keys", p.s("target"), "enter"]))?;
        }
    }
    Ok(json!({}))
}
