# Expiring instructions and status notes

Worker instructions and a task's status note can carry a UTC expiry. The board will not start an agent with an expired instruction or note.

Use set worker.instructions with --expires for instructions. The scope can be your settings, a repo, or one task:

herdr-projects set worker.instructions "Use the local test slot." --expires 2026-10-19T16:32Z --task HOME-5

Use delegate to set the task's instructions and expiry in one request. If you pass only --expires, the board expires the effective instructions for that task without copying them.

herdr-projects delegate HOME-5 --worker.instructions "Use the local test slot." --expires 2026-10-19T16:32Z

Use report to expire a task's status note. --stale-if supports a measured load gate. Once the current load is below the threshold, the note is stale.

herdr-projects report HO-1 --state blocked --msg "Local starts wait for high load." --stale-if "load < 16"

Dates must use UTC and the form YYYY-MM-DDTHH:MMZ or YYYY-MM-DDTHH:MM:SSZ. Use --expires none to clear an instruction expiry. Use --stale-if none to clear a load gate. A report with a new message and no expiry or gate clears its prior expiry and gate. A report that clears the status note also clears its expiry and gate.

The board also recognizes one conservative implicit pattern in instructions: two absolute UTC dates on one line, separated by to, such as 2026-10-19T16:27Z to 2026-10-19T16:32Z. It does not infer a date from clock times such as 16:27-16:32Z.

When you delegate, retry or restart a task with stale instructions or a stale note, the board parks it as waiting. Its reason appears in list and details, followed by renew or clear. Update or remove the stale instruction or note, then delegate again. Expiry does not stop an agent that is already running.
