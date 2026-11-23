// A stand-in for herdr-app's "herdr" module in bun tests of window mods. Bun's mock.module is one per process: the
// first test file to import "herdr" fixes its export names, so a later file's stand-in may only change what they do.
// Every test that mocks "herdr" goes through here, so each one has every name the mods import (none is missing).
const NAMES = [
  "agents", "appHome", "closeOverlay", "commands", "exec", "focused", "hc", "home", "keys", "knownAgents", "layout",
  "layouts", "machines", "nextCommand", "notify", "openOverlay", "panes", "pickFolder", "placement", "playSound", "props",
  "readTextFile", "run", "setShared", "settings", "shared", "spaces", "terminals", "theme", "updates", "watchFolder",
  "window", "writeTextFile",
] as const;

// A mod module keeps what it took from "herdr" when it first loaded (live.js holds spaces() in a closure), and bun
// loads it once per process: a later file's stand-in would never reach it. So every stand-in calls through to the
// latest file's.
let latest: Partial<Record<(typeof NAMES)[number], unknown>> = {};
const NONE = () => undefined;

export function fakeHerdr(given: Partial<Record<(typeof NAMES)[number], unknown>>): Record<string, unknown> {
  latest = given;
  const out: Record<string, unknown> = {};
  for (const n of NAMES) out[n] = (...a: unknown[]) => ((latest[n] ?? NONE) as (...a: unknown[]) => unknown)(...a);
  return out;
}
