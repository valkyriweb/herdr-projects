//! Fake hc protocol transport backed by the shared scripted agent engine, not the runtime adapter.
use super::herdr::FakeHerdr;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use sw::{herdr::{cli::{args, Fake, Ran}, hc}, json::*};
pub struct Core {
    pub engine: Rc<FakeHerdr>,
    pub calls: RefCell<Vec<Vec<String>>>,
    targets: RefCell<HashMap<String, String>>,
    pub app: RefCell<bool>,
    old_path: std::ffi::OsString,
}
pub struct Installed(Rc<Core>);
impl std::ops::Deref for Installed { type Target = Core; fn deref(&self) -> &Core { &self.0 } }
impl Drop for Installed { fn drop(&mut self) { self.0.uninstall(); } }
impl Core {
    pub fn install(w: &super::World) -> Installed {
        let old_path = std::env::var_os("PATH").unwrap_or_default();
        let bin = w.base.join("bin"); std::fs::create_dir_all(&bin).unwrap();
        for name in ["claude", "codex", "pi"] {
            use std::os::unix::fs::PermissionsExt;
            let p = bin.join(name); std::fs::write(&p, "#!/bin/sh\nexit 1\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&old_path))).unwrap();
        unsafe { std::env::set_var("PATH", path); }
        let core = Rc::new(Self { engine: w.fake.clone(), calls: RefCell::new(vec![]), targets: RefCell::new(HashMap::new()), app: RefCell::new(true), old_path });
        hc::set_fake(Some(core.clone())); Installed(core)
    }
    pub fn uninstall(&self) { hc::set_fake(None); unsafe { std::env::set_var("PATH", &self.old_path); } }
    fn pane(&self, name: &str) -> String { self.targets.borrow().get(name).cloned().unwrap_or_default() }
    fn answer(&self, a: &[String]) -> Ran { self.engine.run(a, "") }
    fn value(v: Value) -> Ran { Ran { status: 0, stdout: v.to_string(), stderr: String::new() } }
    pub fn text(&self) -> String { self.calls.borrow().iter().map(|a| a.join(" ")).collect::<Vec<_>>().join("\n") }
}
impl hc::Fake for Core {
    fn run(&self, program: &str, a: &[String]) -> Ran {
        let mut call = vec![program.into()]; call.extend_from_slice(a); self.calls.borrow_mut().push(call);
        let flag = |name: &str| a.iter().position(|s| s == name).and_then(|i| a.get(i+1)).cloned().unwrap_or_default();
        if program == "herdr-app" {
            return if *self.app.borrow() { Self::value(json!({ "spaces": [{ "id": "sp-project", "name": "proj" }] })) }
                else { Ran { status: 1, stdout: String::new(), stderr: "app not running".into() } };
        }
        let target = a.get(1).map(String::as_str).unwrap_or("");
        match a.first().map(String::as_str).unwrap_or("") {
            "ls" => Self::value(json!([])),
            "run" => {
                let name = flag("--name"); let cwd = flag("--cwd");
                let made = { let mut st = self.engine.st.borrow_mut(); FakeHerdr::new_workspace(&mut st, &cwd, Some(&name), Value::Null) };
                let pane = made.at("root_pane").t("pane_id");
                self.targets.borrow_mut().insert(name.clone(), pane.clone());
                let sep = a.iter().position(|s| s == "--").unwrap();
                let pi = (sep+2..a.len()).find(|i| !a[*i].contains('=')).unwrap();
                let kind = std::path::Path::new(&a[pi]).file_name().unwrap().to_str().unwrap();
                let mut start = args(&["agent", "start", &name, "--kind", kind, "--pane", &pane, "--"]);
                start.extend_from_slice(&a[pi+1..]);
                let result = self.answer(&start);
                assert_eq!(result.status, 0, "{}", result.stderr);
                Self::value(json!({ "address": name }))
            }
            "get" => {
                let pane = self.pane(target);
                let r = self.answer(&args(&["agent", "get", &pane]));
                let j: Value = serde_json::from_str(&r.stdout).unwrap_or(Value::Null);
                let agent = j.at("result").at("agent");
                Self::value(json!({ "address": target, "state": if agent.is_null() { "exited" } else { "live" }, "status": { "state": agent.at("agent_status"), "app": agent.at("agent") }, "status_ms": agent.at("state_change_seq"), "turn": agent.at("completion_seq") }))
            }
            "send" => self.answer(&args(&["agent", "prompt", &self.pane(target), &a[2]])),
            "keys" => { let mut k = args(&["agent", "send-keys", &self.pane(target)]); k.extend_from_slice(&a[2..]); self.answer(&k) }
            "interrupt" => self.answer(&args(&["agent", "send-keys", &self.pane(target), "ctrl+c"])),
            "kill" => {
                let id = self.pane(target);
                for p in self.engine.st.borrow_mut()["panes"].as_array_mut().unwrap() { if p.s("pane_id") == id { p["agent"] = Value::Null; } }
                Self::value(json!({}))
            }
            "keep" => Self::value(json!({})),
            "rm" => { self.targets.borrow_mut().remove(target); Self::value(json!({})) }
            "read" => self.answer(&args(&["pane", "read", &self.pane(target), "--source", "visible"])),
            _ => panic!("unmapped fake hc operation {a:?}"),
        }
    }
}
