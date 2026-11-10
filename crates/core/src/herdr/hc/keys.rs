//! hc transport keys: terminal protocol values, not configurable board shortcuts.
/// These fixed terminal sequences interrupt a program independently of the board's chosen stop shortcut.
pub fn interrupt(key: &str) -> bool {
    matches!(key.to_ascii_lowercase().as_str(), "ctrl+c" | "ctrl-c" | "esc" | "escape")
}
