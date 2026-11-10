# herdr-core agent runtime

Set `agents.runtime` to `hc` in your board settings to launch new workers, reviewers, comparison agents and Coordinators in herdr-core terminals. The default, `classic`, preserves the existing herdr runtime. This setting does not move existing agents.

Install hc separately and put it on the board's PATH, or set `HC_BIN_PATH` to its absolute path. The board resolves each agent executable to an absolute path before launching it. No integration or program is installed by the board.

## Places and lifecycle

The board records `hc://local` in the task's `link.socket`. Its `link.pane` and `link.tab` identify a durable board place in the data folder's `hc/` directory. That record holds the hc terminal name, working folder, environment, agent kind and logical task workspace. Fresh board processes use that same mapping. Classic links retain their original socket. Changing the launch setting does not change the classic board socket, hooks or popup routing. A mixed board reads both runtimes; existing task and Coordinator links keep their runtime after a setting change.

| Board operation | hc operation |
|---|---|
| Start worker, reviewer or Coordinator | `hc run --json --bg --no-parent --keep --cwd ... --name ... --label ... -- /usr/bin/env ... <absolute program> <arguments>` |
| Prompt | `hc send`; an unchanged idle terminal gets `hc keys ... enter` to recover a boot-time lost submit. A changed turn or status gets no extra Enter |
| Read output | `hc read ... --lines N` |
| Read state, detect turn end | `hc get`: `status.state`, `status_ms`, `turn` and `address` |
| Stop the current turn | `hc interrupt` |
| End the agent program | `hc keep ... --off`, then `hc kill`; wait for `finished` or disappearance before removal |
| Close task or reviewer terminal | End the program, then `hc rm` |
| Create or remove a worktree | Native `git worktree add/remove`, independent of classic herdr |

Each agent receives `HERDR_PROJECTS_PANE=<board place id>`. Board commands use it with `hc://local` to identify workers, reviewers and Coordinators. This keeps own-task completion guards and autopilot permissions active, and lets a worker use `report` without an id. A classic `HERDR_PANE_ID` takes precedence, even when empty or missing its socket. The hc launch does not set classic `HERDR_PANE_ID` or `HERDR_SOCKET_PATH`; classic socket and popup routing stay unchanged.

The existing keeper polls hc through the same reconciliation path as classic agents. hc turn ends have no classic event hook: the keeper normally polls once per second, with extra delay possible while calls or other work are in progress. The hc address and turn number form the completion identity; `status_ms` orders state readings. Unknown states are not idle. Review, checks and merge decisions remain owned by the existing finish pipeline.

A project reuses an app space by its canonical folder path, regardless of its display name. Name matching is a fallback only for spaces without a folder. A missing space is created through `herdr-app do space.new`. Every terminal has its own stable `herdr.tab` label, and a `herdr-projects.task` label when it belongs to a task. A legacy split request creates another tab, never a grid. If the app cannot answer, the terminal starts without a space label and the reason is appended to `hc/runtime.log`.

Task workspaces are logical ownership groups, not app spaces. Cleanup closes only terminals owned by the task. It never closes the shared project space or another task's terminals.

## Boundaries

This backend does not implement classic popup/plugin panes, terminal takeover, focus/zoom/layout, moving panes, typing an unsubmitted draft, or arbitrary commands typed at a shell prompt. These calls return an explicit unsupported-operation error. Use `hc attach <terminal name>` for interactive terminal access. Classic sidebar metadata is not forwarded; the app uses the terminal labels instead. Renaming a board tab updates its saved label, not the app tab title.

Read-only plan mounts currently require the classic shell-launch operation and are not supported by hc. A guarded planner fails closed instead of running without its sandbox. Background-helper safety relies on hc's state and the board's existing transcript/helper checks; hc does not expose classic foreground-process-group inspection.

The fake-hc tests exercise the transport adapter and the board's delegation, missed-turn reconciliation, review, submission recovery, stop and terminal cleanup. They use scripted agents, not live model sessions. A real hc/app agent launch is still a separate acceptance check.
