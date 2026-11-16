# Upstream provenance

This branch merges `eliasstravik/shepherdr` tag `v3.1.35` into
`valkyriweb/herdr-projects` branch `shep/v3.1.1`, without rewriting the fork's history.
The remote named `origin` is upstream; the remote named `fork` is Luke's fork.
Push this branch only to `fork`.

## Fork changes retained

- `lib/kinds.json` owns the extra agent kinds and ClawRouter model choices.
  `bin/sync-shared` generates `core/kinds.js` and the mods' copies.
- Task launch, model menus, resume/fork handling and session discovery support
  pi, pii, grok, hermes, amp and cursor alongside Claude Code and Codex.
- Tracker session discovery uses the recorded agent kind.
- The Check workflow covers pull requests into `shep/v3.1.1`.

## Local patch carried forward

Workers keep `nice -n 10` but no longer receive the macOS
`taskpolicy -c utility` clamp (`mods/tasks/bin/herdr_task/launch.py`, upstream
issue `eliasstravik/shepherdr#1109`). Linux IO and OOM handling remain unchanged.

## Separate board runtime

Fork PRs #13–#16 changed the Rust board runtime on a different history. They are
not ancestors of `shep/v3.1.1` and are not transplanted into this Python/JS app.
Shepherdr already launches its task agents through herdr-core (`hc`).

## Installation boundary

This update is prepared for review only. Merging or checking this branch does
not authorize installation into either machine's live Herdr home. See
`docs/upgrading.md` for the upstream data migration and rollback contract.
