// redesign-integrate-7: the whole-screen run's outer watchdog
// (scripts/e2e-watchdog.py) kills a run that shows no progress and no
// browser work, names the last case it started, and leaves a run that ends
// on its own alone.
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, writeFileSync, existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { since } from "./support/clock.js";

const WATCHDOG = new URL("../../../scripts/e2e-watchdog.py", import.meta.url)
  .pathname;
const env = {
  ...process.env,
  WATCHDOG_TICK_S: "0.2",
  WATCHDOG_IDLE_S: "1",
  WATCHDOG_CPU_S: "1.0",
};

test("redesign-integrate-7: the watchdog kills a stuck run's whole tree and names its last case", async () => {
  assert.ok(existsSync(WATCHDOG), "no scripts/e2e-watchdog.py");
  const dir = mkdtempSync(join(tmpdir(), "wd-"));
  const log = join(dir, "progress.tap");
  writeFileSync(
    log,
    "TAP version 13\n# Subtest: invariants: a case that hangs\n",
  );
  // The stuck run: a shell whose child sleeps, as node and its browser do.
  const run = spawn("sh", ["-c", "sleep 60 & wait"], { stdio: "ignore" });
  const t0 = performance.now();
  const r = spawnSync("python3", [WATCHDOG, String(run.pid), log], {
    env,
    encoding: "utf8",
    timeout: 15000,
  });
  const took = since(t0);
  assert.equal(r.status, 3, `${r.stdout}\n${r.stderr}`);
  assert.match(
    r.stdout,
    /^watchdog: no progress for \d+ s; last test started: invariants: a case that hangs$/m,
  );
  assert.ok(took < 8000, `the watchdog took ${took} ms`);
  assert.match(readFileSync(`${log}.fired`, "utf8"), /last test started/);
  // The run is gone: ended by the watchdog's kill, not on its own.
  const how = await new Promise((res) => {
    if (run.exitCode !== null || run.signalCode !== null)
      return res(run.signalCode);
    run.on("exit", (_c, sig) => res(sig));
    setTimeout(() => res("still alive"), 3000);
  });
  assert.equal(how, "SIGKILL", "the stuck run was not killed");
});

test("redesign-integrate-7: the watchdog leaves a run that ends on its own alone", () => {
  const dir = mkdtempSync(join(tmpdir(), "wd-"));
  const log = join(dir, "progress.tap");
  writeFileSync(log, "TAP version 13\n");
  const run = spawn("sleep", ["0.5"], { stdio: "ignore" });
  const r = spawnSync("python3", [WATCHDOG, String(run.pid), log], {
    env,
    encoding: "utf8",
    timeout: 15000,
  });
  assert.equal(r.status, 0, `${r.stdout}\n${r.stderr}`);
  assert.ok(!existsSync(`${log}.fired`));
});

test("redesign-integrate-7: the watchdog names the case the harness started even when the runner never reported it", () => {
  const dir = mkdtempSync(join(tmpdir(), "wd-"));
  const log = join(dir, "progress.tap");
  writeFileSync(log, "TAP version 13\n# Subtest: the case before\n");
  writeFileSync(
    `${log}.started`,
    "started: the case before\nstarted: the blocked case\n",
  );
  const run = spawn("sleep", ["30"], { stdio: "ignore" });
  const r = spawnSync("python3", [WATCHDOG, String(run.pid), log], {
    env,
    encoding: "utf8",
    timeout: 15000,
  });
  assert.equal(r.status, 3, `${r.stdout}\n${r.stderr}`);
  assert.match(r.stdout, /last test started: the blocked case$/m);
});
