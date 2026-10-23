//! A program that opens on a dialog only a person can answer (codex's "Hooks need review") is told from its screen, so
//! a review fails at once instead of waiting for its time limit.
use sw::agents::startup_dialog::blocking;

#[test]
fn codex_hook_review_is_a_blocking_dialog() {
    let modal = "  Hooks need review\n  5 hooks are new or changed.\n  Hooks can run outside the sandbox after you trust them.\n› 1. Review hooks\n  2. Trust all and continue\n  enter confirm · esc skip";
    assert!(blocking(modal).unwrap().contains("Trust all"));
    let list = "  ⚠ 5 hooks need review before they can run.\n  t trust all · enter review · esc close";
    assert!(blocking(list).is_some());
}

#[test]
fn a_prompt_at_rest_is_no_dialog() {
    assert_eq!(blocking(">_ OpenAI Codex (v0.164.0)\n› Ask Codex to do anything\n  gpt-6.1-sol medium"), None);
    assert_eq!(blocking(""), None);
    // The phrase alone, as a reviewer's own words might hold it, is no dialog.
    assert_eq!(blocking("Finding: codex shows 'hooks need review' when hooks change"), None);
}
