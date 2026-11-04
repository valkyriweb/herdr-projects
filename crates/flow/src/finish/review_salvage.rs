//! What a review that ran out of time leaves behind: the reviewer's partial findings (the files it saved in the
//! review folder, its last message, the end of its screen) and a structured timeout verdict, kept on the task so that
//! nothing it produced is lost and the next review can take it up where it stopped.
use crate::agents::installed;
use crate::herdr::api::{read_screen, Herdr};
use crate::json::*;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where a timed-out reviewer got to, and the review folder as it left it. Fed by the verdict: `salvage`.
pub struct Left<'a> { pub api: &'a Herdr, pub pane: &'a str, pub kind: &'a str, pub session: &'a str, pub folder: &'a Path, pub dir: &'a Path, pub started_at: i64, pub head: &'a str }
const FILE_CAP: usize = 6000;
/// All of it together: what the task, its history and the next reviewer's brief carry.
const ALL_CAP: usize = 24000;
const MOST_FILES: usize = 24;
const SCREEN_LINES: i64 = 80;
/// The review failed for time: the verdict the task keeps for it. `error` is the review's failure line; `timeout` says
/// which stage ran out of time, after how long, with how much of the reviewer's work in hand; `partial` is that work.
pub fn salvage(l: &Left, error: &str, limit: &str) -> Obj {
    let mut parts: Vec<String> = vec![];
    // The review folder, all of it (screenshots, recordings), moved aside before the next review empties it.
    let kept = keep_folder(l.dir);
    let from = kept.as_deref().unwrap_or(l.dir);
    // The files the reviewer saved in its folder before it ran out of time: notes, evidence, a half-written verdict.
    let mut files: Vec<_> = std::fs::read_dir(from).map(|rd| rd.flatten().filter(|e| e.path().is_file()).collect()).unwrap_or_default();
    files.sort_by_key(|e| e.file_name());
    let more = files.len().saturating_sub(MOST_FILES);
    for e in files.into_iter().take(MOST_FILES) {
        let name = e.file_name().to_string_lossy().into_owned();
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        match head_text(&e.path(), size) {
            Some(body) if body.trim().is_empty() => {}
            Some(body) => parts.push(format!("## {name}\n{body}")),
            None => parts.push(format!("## {name}\n[not text, {size} bytes: kept in the folder below]")),
        }
    }
    if more > 0 { parts.push(format!("[{more} more files: kept in the folder below]")); }
    // Its last message, where its program keeps a conversation the board can read.
    if !l.session.is_empty() {
        if let Some(turn) = installed::read_turn_of(l.kind, None, l.session, l.folder, false) { if turn.pos >= l.started_at && !turn.said.trim().is_empty() { parts.push(format!("## Its last message\n{}", cap(&turn.said, FILE_CAP))); } }
    }
    // The end of its screen, for every program.
    let status = l.api.agent_get(l.pane).ok().flatten().map(|a| a.t("agent_status"));
    let screen = read_screen(l.api, l.pane, status.as_deref(), SCREEN_LINES);
    let screen = super::shared::last_lines(&screen, SCREEN_LINES as usize);
    if !screen.trim().is_empty() { parts.push(format!("## Its screen when it was stopped\n{}", cap(&screen, FILE_CAP))); }
    if let (Some(k), false) = (&kept, parts.is_empty()) { parts.push(format!("## Its folder\n{}", k.display())); }
    // What a screen or a file showed may hold a credential: hidden before the board keeps it or tells it on.
    let partial = cap(&super::redact::hide_secrets(&parts.join("\n\n")), ALL_CAP);
    obj(json!({
        "error": error,
        "timeout": { "stage": "review", "reason": "timeout", "limit": limit, "elapsed": crate::clock::now() - l.started_at, "bytes": partial.len() },
        "partial": partial,
        "head": if l.head.is_empty() { Value::Null } else { json!(l.head) },
        "folder": kept.map(|k| json!(k.display().to_string())).unwrap_or(Value::Null),
        "at": crate::clock::now(),
    }))
}
/// The review folder moved beside it as `review-timed-out-<time>`, when it holds anything: the next review starts empty.
fn keep_folder(dir: &Path) -> Option<PathBuf> {
    if std::fs::read_dir(dir).ok()?.next().is_none() { return None; }
    let name = format!("{}-timed-out-{}", dir.file_name()?.to_string_lossy(), crate::clock::now());
    let to = dir.with_file_name(name);
    std::fs::rename(dir, &to).ok().map(|_| to)
}
/// The start of a file as text, read no further than the cap; None for a file that is not text.
fn head_text(p: &Path, size: u64) -> Option<String> {
    let mut buf = vec![];
    std::fs::File::open(p).ok()?.take(FILE_CAP as u64 + 4).read_to_end(&mut buf).ok()?;
    if buf.contains(&0) { return None; }
    let text = match std::str::from_utf8(&buf) { Ok(t) => t, Err(e) if e.error_len().is_none() => std::str::from_utf8(&buf[..e.valid_up_to()]).ok()?, Err(_) => return None };
    let mut end = text.len().min(FILE_CAP);
    while !text.is_char_boundary(end) { end -= 1; }
    Some(if (size as usize) <= end { text.trim_end().to_string() } else { format!("{}\n[… cut, {size} bytes in all]", text[..end].trim_end()) })
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
