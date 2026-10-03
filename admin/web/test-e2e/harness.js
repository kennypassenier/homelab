// redesign-integrate-7 (coordinator, 2026-10-03: a whole-screen run hung
// 12 minutes on one case and named none): the one place a whole-screen
// case gets its browser. Every context it opens fails a Playwright wait or
// a navigation after STEP_MS unless the case asks for longer, and
// scripts/invariants-run.sh gives every case a deadline of its own, so a
// stalled browser fails fast with the case's name.
// admin/web/test/e2e_deadlines.test.js holds both.
import { appendFileSync } from "node:fs";
import { afterEach, beforeEach } from "node:test";
import { chromium } from "playwright";

/** A Playwright step's default deadline. */
export const STEP_MS = 20000;

/** The browsers a case launched and has not closed yet. */
const open = new Set();

// A case the runner's deadline stopped goes on running in the background:
// its browser is closed after it, so its pending steps fail at once and it
// never holds the run (the 17-minute stall of 2026-10-03).
// The outer watchdog's record of the case that starts now, written before
// its body runs: a case that blocks its own process is still named.
beforeEach((t) => {
  const f = process.env.INVARIANTS_PROGRESS_FILE;
  if (f) appendFileSync(`${f}.started`, `started: ${t.name}\n`);
});

afterEach(async () => {
  const left = [...open];
  open.clear();
  await Promise.all(left.map((b) => b.close().catch(() => {})));
});

/**
 * A headless Chromium whose every context carries the per-step defaults.
 * @param {import("playwright").LaunchOptions} [opts]
 */
export async function launch(opts) {
  const browser = await chromium.launch(opts);
  open.add(browser);
  browser.on("disconnected", () => open.delete(browser));
  const raw = browser.newContext.bind(browser);
  /** @param {import("playwright").BrowserContextOptions} [o] */
  browser.newContext = async (o) => {
    const c = await raw(o);
    c.setDefaultTimeout(STEP_MS);
    c.setDefaultNavigationTimeout(STEP_MS);
    return c;
  };
  /** @param {import("playwright").BrowserContextOptions} [o] */
  browser.newPage = async (o) => (await browser.newContext(o)).newPage();
  return browser;
}
