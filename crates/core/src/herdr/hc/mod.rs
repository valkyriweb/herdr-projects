//! Opt-in herdr-core runtime: stable board places, hc terminals and Herdr app spaces.
pub mod places;
mod commands;
mod keys;
mod programs;
use super::cli::{HRes, HerdrError, Ran};
use crate::json::*;
use std::{cell::RefCell, process::{Command, Stdio}, rc::Rc};

pub const SOCKET: &str = "hc://local";
/// Separate transport fake: hc tests must exercise the adapter, not fake its answers.
pub trait Fake { fn run(&self, program: &str, args: &[String]) -> Ran; }
thread_local! { static FAKE: RefCell<Option<Rc<dyn Fake>>> = const { RefCell::new(None) }; }
pub fn set_fake(f: Option<Rc<dyn Fake>>) { FAKE.with(|x| *x.borrow_mut() = f); }
pub fn selected() -> bool { crate::store::settings::lookup::setting_text("agents.runtime", None) == "hc" }
pub fn launch_socket() -> String { if selected() { SOCKET.into() } else { super::cli::current_socket() } }
pub fn is_socket(socket: Option<&str>) -> bool { socket == Some(SOCKET) }
/// Both runtimes live on this machine, but another classic session remains separate.
pub fn local(socket: &str) -> bool { socket == SOCKET || super::cli::same_socket(socket, &super::cli::current_socket()) }
pub fn error(code: &str, message: impl Into<String>) -> HerdrError { HerdrError { code: code.into(), message: message.into() } }
pub fn raw(program: &str, args: &[String]) -> HRes<String> {
    let fake = FAKE.with(|x| x.borrow().clone());
    let r = if let Some(f) = fake { f.run(program, args) } else {
        let bin = if program == "hc" {
            std::env::var("HC_BIN_PATH").ok().filter(|s| !s.is_empty()).map(Ok).unwrap_or_else(|| {
                programs::executable("hc").or_else(|_| {
                    let p = crate::store::files::user_home().join(".local/bin/hc");
                    if p.is_file() { Ok(p.display().to_string()) } else { Err(error("hc_missing", "hc is not installed: put hc on PATH or set HC_BIN_PATH")) }
                })
            })?
        } else { program.into() };
        let child = Command::new(bin).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| error("spawn_failed", format!("{program}: {e}")))?;
        super::cli::wait_with_timeout(child, 60000)?
    };
    if r.status != 0 { return Err(error("failed", format!("{program}: {}", if r.stderr.is_empty() { r.stdout } else { r.stderr }))); }
    Ok(r.stdout)
}
pub fn json(program: &str, args: &[String]) -> HRes<Value> {
    let text = raw(program, args)?;
    serde_json::from_str(&text).map_err(|e| error("invalid_json", format!("{program}: {e}")))
}
pub fn call(args: &[String]) -> HRes<Value> { commands::call(args) }
pub fn read(args: &[String]) -> HRes<String> {
    let id = args.get(2).map(String::as_str).unwrap_or("");
    let p = places::get(id)?;
    let mut a = super::cli::args(&["read", p.s("target")]);
    if let Some(i) = args.iter().position(|s| s == "--lines") { a.extend(args[i..i+2].iter().cloned()); }
    raw("hc", &a)
}
/// hc status is authoritative; unknown is never treated as idle.
pub fn at_rest(id: &str) -> bool { programs::agent(id).ok().flatten().map(|a| ["idle", "done"].contains(&a.s("agent_status"))).unwrap_or(false) }
pub fn quit(id: &str) -> bool { programs::kill(id).is_ok() }
