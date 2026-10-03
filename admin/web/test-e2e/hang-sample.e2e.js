// redesign-integrate-7: two cases that hang on purpose, to prove the run's
// deadlines bite. They exist only when INVARIANTS_HANG_SAMPLE is set (never
// in a gate run):
//   INVARIANTS_HANG_SAMPLE=1 INVARIANTS_TEST_TIMEOUT_MS=15000 \
//     INVARIANTS_ONLY='/^hang sample/' scripts/invariants-run.sh
// The first must fail by name at the per-test deadline; the second blocks
// the runner itself, so no deadline inside it can fire, and the outer
// watchdog (scripts/e2e-watchdog.py) must kill the run and name it.
import { test } from "node:test";
import { launch } from "./harness.js";

if (process.env.INVARIANTS_HANG_SAMPLE) {
  test("hang sample: a step that never ends fails at the case's deadline", async () => {
    const browser = await launch();
    const page = await (await browser.newContext()).newPage();
    await page.evaluate(() => new Promise(() => {}));
  });

  test("hang sample: a blocked runner is stopped by the outer watchdog", async () => {
    const browser = await launch();
    await (await browser.newContext()).newPage();
    // The test process's own loop never yields: its deadline cannot fire.
    for (;;);
  });
}
