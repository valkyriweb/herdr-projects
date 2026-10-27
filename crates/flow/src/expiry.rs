//! Expiry rules for worker instructions and status notes: exact UTC dates, conservative time windows, and measured gates.
use crate::err::Res;
use crate::json::*;
use crate::store::settings::lookup::{lookup, Scope};
use crate::store::settings::places::yours;
use crate::store::tasks::Task;
use chrono::{NaiveDateTime, TimeZone, Timelike, Utc};

fn timestamp_ms(value: &str) -> Option<i64> {
    let value = value.trim();
    let (format, parser) = match value.len() {
        17 => ("%Y-%m-%dT%H:%MZ", "%Y-%m-%dT%H:%M"),
        20 => ("%Y-%m-%dT%H:%M:%SZ", "%Y-%m-%dT%H:%M:%S"),
        _ => return None,
    };
    let datetime = NaiveDateTime::parse_from_str(value.strip_suffix('Z')?, parser).ok()?;
    (datetime.format(format).to_string() == value).then(|| datetime.and_utc().timestamp_millis())
}
fn timestamp(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms).single().map(|d| {
        if d.second() == 0 { d.format("%Y-%m-%dT%H:%MZ").to_string() }
        else { d.format("%Y-%m-%dT%H:%M:%SZ").to_string() }
    }).unwrap_or_default()
}
/// Check and normalize a UTC expiry supplied as YYYY-MM-DDTHH:MMZ or YYYY-MM-DDTHH:MM:SSZ.
pub fn normalize(value: &str) -> Res<Option<String>> {
    if value.trim().eq_ignore_ascii_case("none") { return Ok(None); }
    let Some(ms) = timestamp_ms(value) else { crate::fail!("--expires needs a UTC date such as 2026-10-19T16:32Z, or none."); };
    Ok(Some(timestamp(ms)))
}
/// Parse the supported measured gate. A load gate is stale once the current load falls below its threshold.
pub fn parse_gate(value: &str) -> Res<Option<Value>> {
    if value.trim().eq_ignore_ascii_case("none") { return Ok(None); }
    let Some((metric, threshold)) = value.split_once('<') else { crate::fail!("--stale-if takes load < threshold, such as load < 16, or none."); };
    let threshold = threshold.trim().parse::<f64>().ok().filter(|n| n.is_finite() && *n >= 0.0);
    if metric.trim() != "load" || threshold.is_none() { crate::fail!("--stale-if takes load < threshold, such as load < 16, or none."); }
    Ok(Some(json!({ "metric": "load", "below": threshold.unwrap() })))
}
fn display_number(value: f64) -> String {
    let s = format!("{value:.1}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
fn load_now() -> Option<f64> {
    let mut loads = [0.0; 1];
    let n = unsafe { libc::getloadavg(loads.as_mut_ptr(), 1) };
    (n == 1).then_some(loads[0])
}
/// An expired explicit date, or an absolute UTC window written as start to end on one line.
fn expired_instruction_date(text: &str, now: i64) -> Option<String> {
    for line in text.lines() {
        let mut moments = vec![];
        for start in 0..line.len() {
            for len in [17, 20] {
                let Some(candidate) = line.get(start..start + len) else { continue };
                if let Some(ms) = timestamp_ms(candidate) { moments.push((start, start + len, ms)); }
            }
        }
        moments.sort_by_key(|(start, _, _)| *start);
        for i in 0..moments.len() {
            for (start, _, at) in moments.iter().skip(i + 1) {
                let (end, begins_at) = (moments[i].1, moments[i].2);
                if end <= *start && begins_at <= *at && line[end..*start].trim() == "to" && *at <= now { return Some(timestamp(*at)); }
            }
        }
    }
    None
}
fn instruction_expiry(task: &Task) -> Option<String> {
    let found = lookup("worker.instructions", &Scope::task(task), &yours()).ok()?;
    if found.value == json!("none") { return None; }
    let now = crate::clock::now();
    let expiries = found.expiries.iter().flat_map(|v| v.as_array().cloned().unwrap_or_else(|| vec![v.clone()])).collect::<Vec<_>>();
    if !expiries.is_empty() {
        return expiries.iter().filter_map(|v| timestamp_ms(&text(v))).filter(|at| *at <= now).min().map(timestamp);
    }
    expired_instruction_date(&text(&found.value), now)
}
/// Current expiry reason, if an instruction, note expiry or measured note gate is stale.
pub fn reason(task: &Task) -> Option<String> {
    if let Some(when) = instruction_expiry(task) { return Some(format!("instructions expired {when}")); }
    if let Some(when) = timestamp_ms(&task.s("noteExpires")).filter(|at| *at <= crate::clock::now()).map(timestamp) {
        return Some(format!("task note expired {when}"));
    }
    let gate = task.at("noteGate");
    if gate.s("metric") == "load" {
        if let (Some(current), Some(threshold)) = (load_now(), gate.at("below").as_f64()) {
            if current < threshold { return Some(format!("stale: load now {} < {}", display_number(current), display_number(threshold))); }
        }
    }
    None
}
/// The task-row text for a stale item that was asked to start.
pub fn waiting(reason: &str) -> String { format!("{reason}: renew or clear") }
/// Put a stale task into the ordinary human-waiting state without starting its agent.
pub fn park(task: &mut Task, reason: &str) {
    task.put("stage", "asks");
    task.put("state", "blocked");
    task.put("kind", "question");
    task.put("msg", waiting(reason));
    task.put("staleCause", reason);
    task.put("turnOpen", false);
}
/// Write the expiry for one setting at the task, user or repo scope.
pub fn set_task_instruction_expiry(task: &mut Task, expiry: Option<&str>) {
    let map = task.obj_mut("settingsExpires");
    match expiry { Some(v) => { map.insert("worker.instructions".into(), json!(v)); } None => { map.shift_remove("worker.instructions"); } }
    if map.is_empty() { task.del("settingsExpires"); }
}
/// Set an optional expiry on a task's status note.
pub fn set_note_expiry(task: &mut Task, expiry: Option<&str>) {
    match expiry { Some(v) => task.put("noteExpires", v), None => task.del("noteExpires") }
}
/// Record or clear the structured gate attached to the status note.
pub fn set_note_gate(task: &mut Task, gate: Option<&Value>) {
    match gate { Some(v) => task.put("noteGate", v.clone()), None => task.del("noteGate") }
}
/// A current load gate's display text, whether or not it is stale.
pub fn note_gate_status(task: &Task) -> Option<String> {
    let gate = task.at("noteGate");
    if gate.s("metric") != "load" { return None; }
    let current = load_now()?;
    let threshold = gate.at("below").as_f64()?;
    Some(format!("load now {} < {}", display_number(current), display_number(threshold)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_explicit_utc_minutes_or_seconds() {
        assert_eq!(normalize("2026-10-19T16:32Z").unwrap(), Some("2026-10-19T16:32Z".into()));
        assert_eq!(normalize("2026-10-19T16:32:04Z").unwrap(), Some("2026-10-19T16:32:04Z".into()));
        assert!(normalize("2026-10-19T16:32+02:00").is_err());
        assert_eq!(normalize("none").unwrap(), None);
    }

    #[test]
    fn parses_only_explicit_utc_windows_that_have_ended() {
        let now = timestamp_ms("2026-10-19T16:33Z").unwrap();
        assert_eq!(expired_instruction_date("Reserved 2026-10-19T16:27Z to 2026-10-19T16:32Z", now), Some("2026-10-19T16:32Z".into()));
        assert_eq!(expired_instruction_date("Ends 2026-10-19T16:34Z", now), None);
        assert_eq!(expired_instruction_date("16:27-16:32Z", now), None);
        assert_eq!(expired_instruction_date("2026-10-19T16:32Z to 2026-10-19T16:27Z", now), None);
    }

    #[test]
    fn parses_load_gates_conservatively() {
        assert_eq!(parse_gate("load < 16").unwrap(), Some(json!({ "metric": "load", "below": 16.0 })));
        assert!(parse_gate("memory < 16").is_err());
        assert!(parse_gate("load < many").is_err());
    }
}
