//! What a review that ran out of time leaves behind: the reviewer's partial findings (the files it saved in the
//! review folder, its last message, the end of its screen) and a structured timeout verdict, kept on the task so that
//! nothing it produced is lost and the next review can take it up where it stopped.
use crate::agents::installed;
use crate::herdr::api::{read_screen, Herdr};
use crate::json::*;
use std::path::Path;

/// Where a timed-out reviewer got to, and the review folder as it left it. Fed by the verdict: `salvage`.
pub struct Left<'a> { pub api: &'a Herdr, pub pane: &'a str, pub kind: &'a str, pub session: &'a str, pub folder: &'a Path, pub dir: &'a Path, pub started_at: i64, pub head: &'a str }
const FILE_CAP: usize = 6000;
const SCREEN_LINES: i64 = 80;
/// The review failed for time: the verdict the task keeps for it. `error` is the review's failure line; `timeout` says
/// which stage ran out of time, after how long, with how much of the reviewer's work in hand; `partial` is that work.
pub fn salvage(l: &Left, error: &str, limit: &str) -> Obj {
    let mut parts: Vec<String> = vec![];
    // The files the reviewer saved in its folder before it ran out of time: notes, evidence, a half-written verdict.
    let mut files: Vec<_> = std::fs::read_dir(l.dir).map(|rd| rd.flatten().filter(|e| e.path().is_file()).collect()).unwrap_or_default();
    files.sort_by_key(|e| e.file_name());
    for e in files {
        let Ok(body) = std::fs::read_to_string(e.path()) else { continue };
        if body.trim().is_empty() { continue; }
        parts.push(format!("## {}\n{}", e.file_name().to_string_lossy(), cap(&body, FILE_CAP)));
    }
    // Its last message, where its program keeps a conversation the board can read.
    if !l.session.is_empty() {
        if let Some(turn) = installed::read_turn_of(l.kind, None, l.session, l.folder, false) { if turn.pos >= l.started_at && !turn.said.trim().is_empty() { parts.push(format!("## Its last message\n{}", cap(&turn.said, FILE_CAP))); } }
    }
    // The end of its screen, for every program.
    let status = l.api.agent_get(l.pane).ok().flatten().map(|a| a.t("agent_status"));
    let screen = read_screen(l.api, l.pane, status.as_deref(), SCREEN_LINES);
    let screen = super::shared::last_lines(&screen, SCREEN_LINES as usize);
    if !screen.trim().is_empty() { parts.push(format!("## Its screen when it was stopped\n{}", cap(&screen, FILE_CAP))); }
    let partial = parts.join("\n\n");
    obj(json!({
        "error": error,
        "timeout": { "stage": "review", "reason": "timeout", "limit": limit, "elapsed": crate::clock::now() - l.started_at, "bytes": partial.len() },
        "partial": partial,
        "head": if l.head.is_empty() { Value::Null } else { json!(l.head) },
        "at": crate::clock::now(),
    }))
}
fn cap(s: &str, n: usize) -> String { if s.len() <= n { s.trim_end().to_string() } else { let mut end = n; while !s.is_char_boundary(end) { end -= 1; } format!("{}\n[… cut, {} bytes in all]", s[..end].trim_end(), s.len()) } }
/// The timed-out review before this one on the same commit, when it left something: what the next reviewer is told.
pub fn resumed_from(t: &Obj, head: &str) -> Option<Value> {
    let hist = crate::store::reviews::review_history(t);
    let e = hist.last()?;
    if !e.at("timeout").is_object() || e.s("partial").trim().is_empty() { return None; }
    if !head.is_empty() && e.b("head") && e.s("head") != head { return None; }
    Some(e.clone())
}
