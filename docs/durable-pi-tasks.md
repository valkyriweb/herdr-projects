# Durable Pi tasks (experimental)

This is **off** unless one task sets `durable=on`. Only a Pi worker can use it. Reviewers and other tasks keep their usual CLI. This flag doesn't change a running worker.

On the canary machine, prepare the separate `pi-durable-task` runner. Install its locked dependencies with `npm ci --ignore-scripts --no-audit --no-fund` and put its executable on PATH. It needs Node 22.19 or newer and a configured, loopback ClawRouter provider in the worker's Pi profile. No global setting or service is needed.

```sh
herdr-task add 'Try durable Pi' --agent pi --model clawrouter/gpt-6.2-sol-200k \
  --effort high --set durable=on --prompt 'Work in this task folder.'
herdr-task start YOUR_TASK_ID
# After the old worker exits or crashes:
herdr-task resume YOUR_TASK_ID
```

Open the task's terminal to read and send messages. The worker has four coding tools and does not load Pi extensions, Pi sessions, or a Shepherdr chat log. It has the same OS permissions as the terminal user, not a separate sandbox. It needs the same working folder, model and effort on recovery. It uses the Pi profile's saved defaults when the task does not specify them. Unsupported CLI arguments and a missing runner are errors, not a silent fallback to Pi.

The task's stable state folder is `pi-durable/<task UUID>/` beside its task list. It holds `task.sqlite` and a separate SQLite owner lock. Keep the whole folder. One runner owns it at a time. Resume resubmits the brief with the same ID, so it can recover even if the first process died just before admission. A crash may cause a new model request. An unfinished Bash call is marked interrupted, not replayed; check its effects before retrying it yourself. A message still in the terminal input buffer is not a saved submission.

To roll back for a task, stop the durable worker, run `herdr-task edit <task> --set durable=off`, and resume with the normal Pi CLI. It starts with a handoff; it does not import the durable transcript or remove the checkpoint. To reopen that checkpoint, stop the current worker and turn the flag back on with the original task settings. Don't keep two workers on the same checkpoint, or move an in-flight checkpoint to another machine.
