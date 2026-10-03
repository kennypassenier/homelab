// redesign-integrate-7 (coordinator, 2026-10-03): a whole-screen run hung
// for 12 minutes on one case and named none. Every case runs under a
// per-test deadline the runner sets, and every browser context it opens
// carries the harness's per-step defaults, so a stalled browser fails fast
// and the failure names its case. This check holds both at commit.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";

const dir = new URL("../test-e2e/", import.meta.url);
const files = readdirSync(dir).filter((f) => f.endsWith(".js"));
const read = (/** @type {string} */ f) => readFileSync(new URL(f, dir), "utf8");

test("redesign-integrate-7: every whole-screen case runs under a per-test deadline and per-step defaults", () => {
  // The runner gives every case a deadline.
  const run = readFileSync(
    new URL("../../../scripts/invariants-run.sh", import.meta.url),
    "utf8",
  );
  // One command per entry, its `\`-continued lines joined.
  const runs = run
    .replace(/\\\n/g, " ")
    .split("\n")
    .filter((l) => /\bnode --test\b/.test(l));
  assert.ok(runs.length > 0, "invariants-run.sh runs no node --test");
  for (const l of runs)
    assert.match(l, /--test-timeout=/, `a run without a deadline: ${l.trim()}`);

  // The outer watchdog watches every run (a hang the deadline cannot see).
  assert.match(run, /e2e-watchdog\.py" "\$run_pid"/, "no watchdog on the run");

  // The harness is the one place a browser is launched, and it gives every
  // context it opens its per-step defaults.
  assert.ok(files.includes("harness.js"), "no test-e2e/harness.js");
  const harness = read("harness.js");
  assert.match(harness, /setDefaultTimeout\(/);
  assert.match(harness, /setDefaultNavigationTimeout\(/);
  // A case that ran out of time leaves no browser working behind it.
  assert.match(harness, /afterEach\(/);
  /** @type {string[]} */
  const bad = [];
  for (const f of files) {
    if (f === "harness.js") continue;
    const src = read(f);
    const line = (/** @type {number | undefined} */ i) =>
      src.slice(0, i).split("\n").length;
    for (const m of src.matchAll(/chromium\s*\.\s*launch\s*\(/g))
      bad.push(`${f}:${line(m.index)}: ${m[0]}`);
    for (const m of src.matchAll(/^import\s*\{[^}]*\}\s*from\s*"playwright"/gm))
      bad.push(`${f}:${line(m.index)}: imports playwright itself`);
    // A wait that names its own deadline must not switch it off.
    for (const m of src.matchAll(/timeout:\s*0\b/g))
      bad.push(`${f}:${line(m.index)}: timeout: 0`);
  }
  assert.deepEqual(
    [...new Set(bad)],
    [],
    "launch browsers through test-e2e/harness.js (launch()), never chromium directly, and never switch a deadline off",
  );
});
