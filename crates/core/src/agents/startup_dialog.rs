//! A dialog a program shows as it starts that only a person can answer, told from its words on the screen: while it
//! shows, nothing typed reaches the program's prompt. Seen on codex 0.164.0: after ~/.codex/hooks.json changes, every
//! new session opens on "Hooks need review" until the hooks are trusted, and herdr reports the pane idle, not blocked.

/// The words the screen shows (all of them), and what a person does about it. One entry per dialog.
const DIALOGS: &[(&[&str], &str)] = &[
    (&["hooks need review", "trust all"], "codex shows its \"Hooks need review\" dialog: its hooks changed and wait to be trusted. Start codex once in a terminal and choose \"Trust all and continue\" (or press t on its Hooks list), then retry"),
];

/// The dialog on this screen that blocks the program, said for a person, or None.
pub fn blocking(screen: &str) -> Option<&'static str> {
    let s = screen.to_lowercase();
    DIALOGS.iter().find(|(words, _)| words.iter().all(|w| s.contains(w))).map(|(_, say)| *say)
}
