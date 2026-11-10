//! Translate the board's existing place and lifecycle operations to hc, leaving classic calls untouched.
use super::{error, places, programs, raw};
use crate::{herdr::cli::{args, HRes}, json::*};
fn at(a: &[String], n: usize) -> &str { a.get(n).map(String::as_str).unwrap_or("") }
fn flag<'a>(a: &'a [String], key: &str) -> &'a str { a.iter().position(|s| s == key).map(|i| at(a, i+1)).unwrap_or("") }
fn envs(a: &[String]) -> Vec<String> { a.windows(2).filter(|w| w[0] == "--env").map(|w| w[1].clone()).collect() }
fn git(root: &str, a: &[&str]) -> HRes<String> {
    let out = std::process::Command::new("git").args(["-C", root]).args(a).output().map_err(|e| error("git_failed", e.to_string()))?;
    if !out.status.success() { return Err(error("git_failed", String::from_utf8_lossy(&out.stderr).into_owned())); }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
fn worktree(a: &[String]) -> HRes<Value> {
    super::json("hc", &args(&["ls", "--all", "--json"]))?;
    let root = flag(a, "--cwd");
    let (cwd, branch) = if at(a, 1) == "create" {
        let branch = flag(a, "--branch");
        let cwd = crate::store::files::data_dir().join("worktrees").join(format!("hc-{}", crate::util::random_hex(10)));
        std::fs::create_dir_all(cwd.parent().unwrap()).map_err(|e| error("store_failed", e.to_string()))?;
        let path = cwd.display().to_string();
        git(root, &["worktree", "add", "-b", branch, &path, if flag(a, "--base").is_empty() { "HEAD" } else { flag(a, "--base") }])?;
        (path, branch.to_string())
    } else { let cwd = flag(a, "--path").to_string(); let branch = git(&cwd, &["branch", "--show-current"])?; (cwd, branch) };
    places::create(root, &cwd, flag(a, "--label"), envs(a), json!({ "checkout_path": cwd, "repo_root": root, "is_linked_worktree": true, "branch": branch }))
}
pub fn call(a: &[String]) -> HRes<Value> {
    let (area, op, id) = (at(a, 0), at(a, 1), at(a, 2));
    match (area, op) {
        ("workspace", "list") => {
            // Probe the runtime even if the board has no saved places yet.
            super::json("hc", &args(&["ls", "--all", "--json"]))?;
            Ok(json!({ "workspaces": places::all().iter().filter(|p| p.s("type") == "workspace").map(places::workspace).collect::<Vec<_>>() }))
        }
        ("workspace", "get") => Ok(json!({ "workspace": places::workspace(&places::get(id)?) })),
        ("workspace", "create") => places::create(flag(a, "--cwd"), flag(a, "--cwd"), flag(a, "--label"), envs(a), Value::Null),
        ("workspace", "close") => places::close(id),
        ("worktree", "create" | "open") => worktree(a),
        ("worktree", "remove") => {
            let ws = flag(a, "--workspace"); let w = places::get(ws)?;
            places::close(ws)?;
            let mut g = vec!["worktree", "remove"];
            if a.iter().any(|s| s == "--force") { g.push("--force"); }
            g.push(w.s("cwd")); git(w.s("root"), &g)?; Ok(json!({}))
        }
        ("pane", "list") | ("tab", "list") => {
            let ws = flag(a, "--workspace");
            let rows: Vec<Value> = places::all().iter().filter(|p| p.s("type") == "pane" && (ws.is_empty() || p.s("workspace") == ws)).map(if area == "pane" { places::pane } else { places::tab }).collect();
            Ok(if area == "pane" { json!({ "panes": rows }) } else { json!({ "tabs": rows }) })
        }
        ("pane", "get") => Ok(json!({ "pane": places::pane(&places::get(id)?) })),
        ("tab", "get") => Ok(json!({ "tab": places::tab(&places::get(id)?) })),
        ("tab", "create") => {
            let p = places::new_pane(flag(a, "--workspace"), flag(a, "--cwd"), flag(a, "--label"), envs(a))?;
            Ok(json!({ "tab": places::tab(&p), "root_pane": places::pane(&p) }))
        }
        ("pane", "split") => {
            // Even legacy split callers get a tab, never a grid in the Herdr app.
            let old = places::get(id)?;
            let p = places::new_pane(old.s("workspace"), flag(a, "--cwd"), "", envs(a))?;
            Ok(json!({ "pane": places::pane(&p) }))
        }
        ("pane", "close") | ("tab", "close") => places::close(id),
        ("tab", "rename") => { let mut p = places::get(id)?; p["label"] = json!(at(a, 3)); places::save(&p)?; Ok(json!({})) }
        ("agent", "list") => {
            let mut agents = vec![];
            for p in places::all().iter().filter(|p| p.s("type") == "pane") { if let Some(a) = programs::agent(p.s("id"))? { agents.push(a); } }
            Ok(json!({ "agents": agents }))
        }
        ("agent", "get") => Ok(json!({ "agent": programs::agent(id)? })),
        ("agent", "start") => {
            let extra = a.iter().position(|s| s == "--").map(|i| &a[i+1..]).unwrap_or(&[]);
            Ok(json!({ "agent": programs::start(flag(a, "--pane"), id, flag(a, "--kind"), extra)? }))
        }
        ("agent", "prompt") => programs::prompt(id, at(a, 3)),
        ("agent", "send-keys") => {
            let p = places::get(id)?;
            let keys = &a[3..];
            if keys.len() == 1 && ["ctrl-c", "esc"].contains(&keys[0].replace('+', "-").as_str()) { raw("hc", &args(&["interrupt", p.s("target")]))?; }
            else { let mut k = args(&["keys", p.s("target")]); k.extend_from_slice(keys); raw("hc", &k)?; }
            Ok(json!({}))
        }
        ("pane", "report-metadata") | ("workspace", "report-metadata") => Ok(json!({})),
        _ => Err(error("unsupported_hc", format!("hc runtime does not support {area} {op}; use hc attach for terminal access"))),
    }
}
