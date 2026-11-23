import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { Model, runsCommand } from "./model";

// #1022: a worker that died mid-turn leaves its plain shell, and hc keeps "working" on it: the web app's Tasks
// list counted it as the task's agent, the task Working for good. A shell that runs its agent (fish -ic, #780) stays.
describe("a task's agents leave out a dead agent's shell (as herdr-task's runs_command)", () => {
  test("a plain shell runs no command", () => {
    expect(runsCommand(["/bin/bash", "-i"])).toBe(false);
    expect(runsCommand(["zsh"])).toBe(false);
    expect(runsCommand(["-bash", "-l"])).toBe(false);
  });
  test("a shell running the agent's launch command does", () => {
    expect(runsCommand(["/bin/bash", "-ic", "\"$@\"; exec \"$0\" -i", "/bin/bash", "codex"])).toBe(true);
    expect(runsCommand(["fish", "-ic", "claude go; exec fish -i"])).toBe(true);
    expect(runsCommand(["fish", "--command=claude"])).toBe(true);
  });
  test("its argv unknown: its status decides, as before", () => {
    expect(runsCommand(undefined)).toBe(true);
  });
});

describe("a remote lane belongs to its home task list", () => {
  const list = { slug: "probe", machine: null };
  const task = { id: 3, lane: { machine: "worker" } };
  const terminal = (address: string, role = "worker", parent: string | null = null) => ({
    address, state: "live", labels: { "herdr.task": "probe#3", "herdr.role": role }, parent,
    status: { state: "working" }, foreground: { name: "pi" },
  });
  const terms = [terminal("t1"), terminal("worker/t1"), terminal("other/t1"), terminal("worker/t2", "helper", "t1"), terminal("t2", "helper", "t1")];
  const model: any = new Model({ terminals: new Map(terms.map((item) => [item.address, item])) } as any, {} as any);

  test("the web finds the lane worker and its helper without moving the task list", () => {
    const live = model.agentsOf(list, task);
    expect(live.map((item: any) => item.address)).toEqual(["worker/t1"]);
    expect(model.helpersOf(list, task, live).map((item: any) => item.address)).toEqual(["worker/t2"]);
    expect(list.machine).toBeNull();
  });

  test("local overrides and other machines' lists keep matching their own agents", () => {
    expect(model.agentsOf(list, { id: 3 }).map((item: any) => item.address)).toEqual(["t1"]);
    expect(model.agentsOf({ ...list, machine: "other" }, { id: 3 }).map((item: any) => item.address)).toEqual(["other/t1"]);
  });

  test("the window uses the lane machine too, without taking a namesake from another machine", () => {
    const source = readFileSync(new URL("../mods/tasks/main.js", import.meta.url), "utf8");
    const methods = ["live(list, task)", "helpers(list, task, live)"].map((signature) => {
      const start = source.indexOf(`  ${signature} {`);
      return source.slice(start, source.indexOf("\n  }", start) + 4);
    });
    const panel = new Function("agents", "terminals", "panes", "chatPane", "helper", "machineOf", "RANK",
      `return { ${methods.join(",")} };`)(() => [], () => terms, () => [], () => null,
      (item: any) => item.labels["herdr.role"] === "helper", (address: string) => address.includes("/") ? address.split("/")[0] : null, { working: 2 });
    const live = panel.live(list, task);
    expect(live.map((item: any) => item.address)).toEqual(["worker/t1"]);
    expect(panel.helpers(list, task, live).map((item: any) => item.address)).toEqual(["worker/t2"]);
    expect(panel.live(list, { id: 3 }).map((item: any) => item.address)).toEqual(["t1"]);
  });

  test("the window's reverse lookup keeps the lane's task record on its home machine", () => {
    const source = readFileSync(new URL("../mods/tasks/main.js", import.meta.url), "utf8");
    const signatures = ["listForAgent(address)", "listOf(slug, machine)", "taskRef(address)", "taskTitle(address)"];
    const methods = signatures.map((signature) => {
      const start = source.indexOf(`  ${signature} {`);
      if (start < 0) throw new Error(`Missing ${signature}`);
      return source.slice(start, source.indexOf("\n  }", start) + 4);
    });
    const panel = new Function("terminals", "machineOf", "refOf", `return { ${methods.join(",")} };`)(
      () => terms, (address: string) => address.includes("/") ? address.split("/")[0] : null,
      (record: any, item: any) => `${record.prefix}-${item.id}`);
    const home = { ...list, prefix: "HOME", tasks: [{ ...task, title: "Remote work", agents: [{ address: "worker/t1" }] }] };
    panel.lists = [home, { ...list, machine: "worker", prefix: "NATIVE", tasks: [{ id: 3 }] }];
    expect(panel.listForAgent("worker/t1")).toBe(home);
    expect(panel.taskRef("worker/t1")).toBe("HOME-3");
    expect(panel.taskTitle("worker/t1")).toBe("Remote work");
    expect(panel.listForAgent("t1")).toBe(home);
  });

  test("the window task header reads the supplied home key rather than the worker's machine", async () => {
    const source = readFileSync(new URL("../mods/chat/panel.js", import.meta.url), "utf8");
    const start = source.indexOf("  async loadTask(addr, cx) {");
    const method = source.slice(start, source.indexOf("\n  }", start) + 4);
    const reads: string[] = [];
    const panel = new Function("exec", "readTextFile", "dirs", "SH", `return { ${method} };`)(
      async () => { throw new Error("Must not read the lane machine's task store"); },
      async (path: string) => { reads.push(path); return JSON.stringify({ folder: "/repo", prefix: "HOME", tasks: [{ id: 3, state: "ready" }] }); },
      () => ({ tasks: "/tasks" }), "");
    panel.cur = { key: { machine: null, slug: "probe", id: 3 } };
    panel.taskOf = () => ({ machine: "worker", slug: "probe", id: 3 });
    await panel.loadTask("worker/t1", { notify() {} });
    expect(reads).toEqual(["/tasks/probe.json"]);
    expect(panel.rec.ref).toBe("HOME-3");
    expect(panel.rec.machine).toBeNull();
  });
});
