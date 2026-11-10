//! Fake hc protocol transport backed by the shared scripted agent engine, not the runtime adapter.
use super::herdr::FakeHerdr;
use std::{cell::RefCell, collections::HashMap, rc::Rc};
use sw::{herdr::{cli::{args, Fake, Ran}, hc}, json::*};
pub struct Core {
    pub engine: Rc<FakeHerdr>,
    pub calls: RefCell<Vec<Vec<String>>>,
    targets: RefCell<HashMap<String, String>>,
    pub app: RefCell<bool>,
    pub spaces: RefCell<Value>,
    pub get_override: RefCell<Option<Value>>,
    pub kill_lag: std::cell::Cell<u32>,
    /// 1: the program has no hc status yet (still starting) until the first send, and for one look after it.
    pub unread: std::cell::Cell<u8>,
    pending_kills: RefCell<HashMap<String, u32>>,
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
        let core = Rc::new(Self { engine: w.fake.clone(), calls: RefCell::new(vec![]), targets: RefCell::new(HashMap::new()), app: RefCell::new(true), spaces: RefCell::new(json!([{ "id": "sp-project", "name": "Custom cockpit", "folder": w.root_str() }])), get_override: RefCell::new(None), kill_lag: std::cell::Cell::new(0), unread: std::cell::Cell::new(0), pending_kills: RefCell::new(HashMap::new()), old_path });
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
            return if *self.app.borrow() { Self::value(json!({ "spaces": self.spaces.borrow().clone() })) }
                else { Ran { status: 1, stdout: String::new(), stderr: "app not running".into() } };
        }
        let target = a.get(1).map(String::as_str).unwrap_or("");
        match a.first().map(String::as_str).unwrap_or("") {
            "ls" => Self::value(json!([])),
            "run" => {
                assert!(a.iter().any(|s| s == "--json"), "hc run defaults to text, even when piped");
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
                if let Some(v) = self.get_override.borrow().as_ref() { return Self::value(v.clone()); }
                match self.unread.get() {
                    1 => return Self::value(json!({ "address": target, "state": "live", "status": null, "status_ms": 0 })),
                    2 => { self.unread.set(0); return Self::value(json!({ "address": target, "state": "live", "status": null, "status_ms": 0 })); }
                    _ => {}
                }
                let pane = self.pane(target);
                if let Some(n) = self.pending_kills.borrow_mut().get_mut(target) {
                    if *n == 0 { for p in self.engine.st.borrow_mut()["panes"].as_array_mut().unwrap() { if p.s("pane_id") == pane { p["agent"] = Value::Null; } } }
                    else { *n -= 1; }
                }
                let r = self.answer(&args(&["agent", "get", &pane]));
                let j: Value = serde_json::from_str(&r.stdout).unwrap_or(Value::Null);
                let agent = j.at("result").at("agent");
                Self::value(json!({ "address": target, "state": if agent.is_null() { "finished" } else { "live" }, "status": if agent.is_null() { Value::Null } else { json!({ "state": agent.at("agent_status"), "app": agent.at("agent") }) }, "status_ms": 1_790_000_000_000i64 + agent.i("state_change_seq"), "turn": agent.i("completion_seq") }))
            }
            "send" => { if self.unread.get() == 1 { self.unread.set(2); } self.answer(&args(&["agent", "prompt", &self.pane(target), &a[2]])) }
            "keys" => { let mut k = args(&["agent", "send-keys", &self.pane(target)]); k.extend_from_slice(&a[2..]); self.answer(&k) }
            "interrupt" => self.answer(&args(&["agent", "send-keys", &self.pane(target), "ctrl+c"])),
            "kill" => {
                let id = self.pane(target);
                if self.kill_lag.get() > 0 { self.pending_kills.borrow_mut().insert(target.into(), self.kill_lag.get()); }
                else { for p in self.engine.st.borrow_mut()["panes"].as_array_mut().unwrap() { if p.s("pane_id") == id { p["agent"] = Value::Null; } } }
                Self::value(json!({}))
            }
            "keep" => Self::value(json!({})),
            "rm" => {
                let id = self.pane(target);
                assert!(!self.engine.st.borrow().arr("panes").iter().any(|p| p.s("pane_id") == id && p.at("agent").is_object()), "hc rm only accepts finished terminals");
                self.pending_kills.borrow_mut().remove(target); self.targets.borrow_mut().remove(target); Self::value(json!({}))
            }
            "read" => self.answer(&args(&["pane", "read", &self.pane(target), "--source", "visible"])),
            _ => panic!("unmapped fake hc operation {a:?}"),
        }
    }
}
