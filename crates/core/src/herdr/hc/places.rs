//! Durable logical places: each board workspace owns only its terminals, never a shared app space.
use super::{error, json as run_json};
use crate::{herdr::cli::{args, HRes}, json::*, store::files::{data_dir, read_json, write_json}};
use std::path::{Path, PathBuf};
fn dir() -> PathBuf { data_dir().join("hc") }
fn file(id: &str) -> HRes<PathBuf> {
    if !id.starts_with("hc-") || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') { return Err(error("not_found", "Unknown hc board place")); }
    Ok(dir().join(format!("{id}.json")))
}
pub fn get(id: &str) -> HRes<Value> { read_json(&file(id)?).ok_or_else(|| error("not_found", format!("hc board place {id} is gone"))) }
pub fn save(v: &Value) -> HRes<()> { write_json(&file(v.s("id"))?, v).map_err(|e| error("store_failed", e.msg)) }
pub fn all() -> Vec<Value> {
    std::fs::read_dir(dir()).into_iter().flatten().flatten().filter_map(|e| read_json(&e.path())).collect()
}
pub fn forget(id: &str) -> HRes<()> { std::fs::remove_file(file(id)?).map_err(|e| error("store_failed", e.to_string())) }
fn new_id() -> String { format!("hc-{}", crate::util::random_hex(10)) }
pub fn pane(p: &Value) -> Value { json!({ "pane_id": p.at("id"), "tab_id": p.at("id"), "workspace_id": p.at("workspace"), "terminal_id": p.at("id"), "cwd": p.at("cwd"), "foreground_cwd": p.at("cwd") }) }
pub fn workspace(w: &Value) -> Value { json!({ "workspace_id": w.at("id"), "label": w.at("label"), "cwd": w.at("cwd"), "worktree": w.at("worktree") }) }
pub fn tab(p: &Value) -> Value { json!({ "tab_id": p.at("id"), "workspace_id": p.at("workspace"), "label": p.at("label") }) }
fn app_space(root: &str) -> String {
    let name = Path::new(root).file_name().unwrap_or_default().to_string_lossy().into_owned();
    let find = |v: &Value| {
        let same_folder = |s: &&Value| std::fs::canonicalize(s.s("folder")).ok().zip(std::fs::canonicalize(root).ok()).map(|(a,b)| a == b).unwrap_or_else(|| s.s("folder") == root);
        v.arr("spaces").iter().find(same_folder)
            .or_else(|| v.arr("spaces").iter().find(|s| s.s("name") == name && s.s("folder").is_empty()))
            .map(|s| s.t("id"))
    };
    let result = (|| -> HRes<String> {
        // Serialize only the app's lookup/create pair, not the board's agent launches.
        let _guard = crate::store::files::hold_lock("hc-app-space", 10000).map_err(|e| error("lock_failed", e.msg))?;
        if let Some(id) = find(&run_json("herdr-app", &args(&["ls"]))?) { return Ok(id); }
        run_json("herdr-app", &args(&["do", "space.new", "--name", &name, "--folder", root]))?;
        find(&run_json("herdr-app", &args(&["ls"]))?).ok_or_else(|| error("not_found", "Herdr app did not expose the new space"))
    })();
    result.unwrap_or_else(|e| {
        use std::io::Write;
        let _ = std::fs::create_dir_all(dir());
        let _ = std::fs::OpenOptions::new().create(true).append(true).open(dir().join("runtime.log")).and_then(|mut f| writeln!(f, "{}: starting hc without an app space: {e}", crate::clock::iso(crate::clock::now())));
        String::new()
    })
}
pub fn create(root: &str, cwd: &str, label: &str, env: Vec<String>, worktree: Value) -> HRes<Value> {
    run_json("hc", &args(&["ls", "--all", "--json"]))?;
    let id = new_id();
    let w = json!({ "id": id, "type": "workspace", "cwd": cwd, "root": root, "label": label, "space": app_space(root), "worktree": worktree });
    save(&w)?;
    let p = new_pane(&id, cwd, label, env)?;
    Ok(json!({ "workspace": workspace(&w), "tab": tab(&p), "root_pane": pane(&p), "worktree": { "path": cwd } }))
}
pub fn new_pane(ws: &str, cwd: &str, label: &str, env: Vec<String>) -> HRes<Value> {
    let w = get(ws)?;
    let id = new_id();
    let p = json!({ "id": id, "type": "pane", "target": id, "workspace": ws, "cwd": cwd, "root": w.at("root"), "space": w.at("space"), "label": label, "env": env });
    save(&p)?; Ok(p)
}
pub fn close(id: &str) -> HRes<Value> {
    let v = get(id)?;
    if v.s("type") == "workspace" {
        for p in all().iter().filter(|p| p.s("workspace") == id) { close(p.s("id"))?; }
    } else if v.b("started") { super::programs::remove(id)?; }
    forget(id)?;
    Ok(json!({}))
}
