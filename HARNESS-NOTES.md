# Harness handoff (stopped)
Read-only discovery completed; no implementation changes. Existing baseline: 20 tests OK (1 skipped); sync-shared OK. Coordinator owns further work.
pi / pii: help confirms --model provider/id, --thinking off|minimal|low|medium|high|xhigh (also max), --session path|id, --continue, --fork path|id; read-only example: --tools read,grep,find,ls. Session location ~/.pi/agent/sessions/<cwd-slug>/*.jsonl comes from brief, not independently inspected.
claude: existing integration uses --model, --effort, --resume, --fork-session, --permission-mode plan; session pointer ~/.claude/sessions/<pid>.json and logs ~/.claude/projects/*/<sessionId>.jsonl (source inspected; CLI help not checked).
codex: existing integration uses -m, -c model_reasoning_effort=VALUE, resume/fork subcommands, -s read-only -a on-request; logs rollout-*.jsonl found via process file descriptors (source inspected; CLI help not checked).
grok: help confirms --model/-m, --reasoning-effort/--effort, --resume/-r [id|title], --continue, --session-id, --fork-session; --permission-mode plan and --tools available; storage unverified.
hermes: help confirms chat -m/--model, --reasoning, --resume, --continue, -q/--query initial prompt; --toolsets supported, no verified read-only toolset; storage unverified.
amp: help confirms --mode/-m low|medium|high|ultra selects model/system/tools (no --model); threads continue and last subcommands; prompt via stdin or --execute; no verified read-only mode or storage.
cursor-agent: help confirms --model, bracket effort overrides, --resume [chatId], --continue, --mode plan|ask (read-only); storage unverified.
Integration owners: lib/agent.js/settings.js/machines.js/inbox.js; lib/COPIES + bin/sync-shared; mods/chat/{main,log,screens}.js; mods/chat-models/main.js; mods/tasks/bin/herdr_task/{settings,launch,sessions,takeover,machine}.py; bin/shepherdr hooks.
