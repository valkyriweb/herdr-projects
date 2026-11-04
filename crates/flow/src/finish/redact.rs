//! Hides what looks like a credential in text the board keeps from a screen or a file it did not write (a timed-out
//! reviewer's partial findings, finish/review_salvage.rs): bearer and basic headers, key=value secrets, private keys,
//! and the token shapes of the common services. A best effort: what it does not recognise is kept as it was.
use std::sync::LazyLock;

const HIDDEN: &str = "[hidden]";
static WHOLE: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(concat!(
    r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?(?:-----END [A-Z ]*PRIVATE KEY-----|\z)",
    r"|\b(?:sk|pk|rk)-[A-Za-z0-9_-]{16,}",
    r"|\bgh[pousr]_[A-Za-z0-9]{20,}|\bgithub_pat_[A-Za-z0-9_]{20,}|\bglpat-[A-Za-z0-9_-]{20,}",
    r"|\bxox[abprs]-[A-Za-z0-9-]{10,}|\bAKIA[0-9A-Z]{16}\b|\bAIza[0-9A-Za-z_-]{30,}",
    r"|\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}",
)).unwrap());
static HEADER: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(?i)(\b(?:bearer|basic|token)\s+)[A-Za-z0-9._~+/=-]{8,}").unwrap());
static NAMED: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(concat!(
    r#"(?i)(\b[A-Za-z0-9_.-]*(?:api[_-]?key|secret|token|password|passwd|pwd|access[_-]?key|private[_-]?key|auth)[A-Za-z0-9_.-]*["']?\s*[:=]\s*["']?)([^\s"',;]{6,})"#,
)).unwrap());
static IN_URL: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"(://[^/\s:@]+:)[^/\s@]+@").unwrap());

/// The text with every credential it recognises replaced by "[hidden]".
pub fn hide_secrets(s: &str) -> String {
    let s = WHOLE.replace_all(s, HIDDEN);
    let s = HEADER.replace_all(&s, |c: &regex::Captures| format!("{}{HIDDEN}", &c[1]));
    // "Authorization: Bearer [hidden]": the scheme's name is no secret, nor what is already hidden.
    let s = NAMED.replace_all(&s, |c: &regex::Captures| {
        let v = &c[2];
        if v.starts_with("[hidden") || ["bearer", "basic", "token"].contains(&v.to_lowercase().as_str()) { c[0].to_string() } else { format!("{}{HIDDEN}", &c[1]) }
    });
    IN_URL.replace_all(&s, |c: &regex::Captures| format!("{}{HIDDEN}@", &c[1])).into_owned()
}
