// redesign-integrate-7 (coordinator, 2026-10-03: a whole-screen run hung
// 12 minutes on one case and named none): the one place a whole-screen
// case gets its browser. Every context it opens fails a Playwright wait or
// a navigation after STEP_MS unless the case asks for longer, and
// scripts/invariants-run.sh gives every case a deadline of its own, so a
// stalled browser fails fast with the case's name.
// admin/web/test/e2e_deadlines.test.js holds both.
import { appendFileSync } from "node:fs";
import { afterEach, beforeEach } from "node:test";
import { firefox } from "playwright";
import { clockNow } from "./clock.js";

/** A Playwright step's default deadline. */
export const STEP_MS = 20000;

/** The contexts a case opened and has not closed yet. */
const openContexts = new Set();

/** @type {Promise<import("playwright").Browser> | null} the file's one Firefox */
let shared = null;

// feat-kpadopt-2 (Kenny's choice of 2026-10-22, "one-firefox"): the cases of
// a file share ONE Firefox process. Launching one per case cost 2-3 s a
// case; now a case gets its own fresh context (own cookies, storage and
// preferences; the stored preferences are wiped per case in
// invariants.e2e.js) on the process the file's first case started.
// `browser.close()` in a case is a no-op until the file ends.
// A case the runner's deadline stopped goes on running in the background:
// its contexts are closed after it, so its pending steps fail at once and it
// never holds the run (the 17-minute stall of 2026-10-03).
// The outer watchdog's record of the case that starts now, written before
// its body runs: a case that blocks its own process is still named.
beforeEach((t) => {
  const f = process.env.INVARIANTS_PROGRESS_FILE;
  if (f) appendFileSync(`${f}.started`, `started: ${t.name}\n`);
});

afterEach(async () => {
  const left = [...openContexts];
  openContexts.clear();
  await Promise.all(left.map((c) => c.close().catch(() => {})));
});

// The file's end: the process ends (the runner's --test-force-exit) and takes
// the Playwright driver and its one Firefox with it; a root-level `after`
// hook would make a file without cases fail under --test-force-exit.
process.once("beforeExit", async () => {
  const b = shared ? await shared.catch(() => null) : null;
  shared = null;
  await b?.close().catch(() => {});
});

/** The real browser: the file's one Firefox, started on first use and again if it died. */
async function realBrowser() {
  if (shared) {
    const b = await shared.catch(() => null);
    if (b?.isConnected()) return b;
    shared = null;
  }
  // INVARIANTS_FIREFOX: a Firefox build to use when the one this Playwright
  // wants is not downloaded (the same protocol works across a few builds).
  const executablePath = process.env.INVARIANTS_FIREFOX || undefined;
  shared = firefox.launch({ ...(executablePath && { executablePath }) });
  return shared;
}

/**
 * A headless Firefox (Kenny: browser tests run on Firefox only, never
 * Chromium) whose every context carries the per-step defaults. The process
 * is the file's one shared Firefox; this returns a view of it whose
 * `close()` does nothing.
 */
export async function launch() {
  const real = await realBrowser();
  const view = new Proxy(real, {
    get(target, prop) {
      if (prop === "close") return async () => {};
      if (prop === "newContext") return newContext;
      if (prop === "newPage")
        return async (/** @type {any} */ o) => (await newContext(o)).newPage();
      const v = Reflect.get(target, prop, target);
      return typeof v === "function" ? v.bind(target) : v;
    },
  });
  /** @param {import("playwright").BrowserContextOptions} [o] */
  async function newContext(o) {
    const c = await real.newContext(o);
    openContexts.add(c);
    c.on("close", () => openContexts.delete(c));
    c.setDefaultTimeout(STEP_MS);
    c.setDefaultNavigationTimeout(STEP_MS);
    // redesign-final (B): the page's clock is the run's injected clock
    // (clock.js), the one the demo host counts from; time runs on.
    // Only the date: the page's own timers run as they do for a person.
    if (process.env.INVARIANTS_NO_CLOCK !== "1")
      await c.clock.setSystemTime(clockNow());
    return c;
  }
  return view;
}

/**
 * Replaces a fixed `waitForTimeout(ms)` that waited for a page to finish
 * drawing: resolves, after 250 ms at the least, as soon as no web font is
 * loading, no finite animation or transition is running, and the layout (the
 * rectangles of the page's elements) is the same over two consecutive
 * frames; never later than `cap` ms. The same checks as the sleep it
 * replaces, in the time the page actually needs.
 * @param {import("playwright").Page} page
 * @param {number} [cap] hard limit in ms: the sleep it replaces, so it is never slower (default 2500)
 */
export async function settle(page, cap = 2500) {
  const left = cap;
  await page
    .evaluate(async (limit) => {
      const t0 = performance.now();
      const end = t0 + limit;
      const frame = () => new Promise((r) => requestAnimationFrame(() => r(0)));
      const sig = () => {
        let s = `${document.documentElement.scrollWidth}x${document.documentElement.scrollHeight}`;
        let n = 0;
        for (const el of document.querySelectorAll("body *")) {
          if (n++ > 1500) break;
          const r = el.getBoundingClientRect();
          s += `|${Math.round(r.x)},${Math.round(r.y)},${Math.round(r.width)},${Math.round(r.height)}`;
        }
        return s;
      };
      const busy = () =>
        performance.now() - t0 < 250 ||
        document.fonts.status === "loading" ||
        document.getAnimations().some((a) => {
          const t = a.effect?.getComputedTiming();
          return (
            a.playState === "running" &&
            t &&
            Number.isFinite(t.endTime) &&
            // an ambient indicator (a 60 s refresh countdown) is not settling
            t.endTime - (a.currentTime ?? 0) < 2000 &&
            (a.currentTime ?? 0) < t.endTime
          );
        });
      await document.fonts.ready.catch(() => {});
      let prev = "";
      let same = 0;
      while (performance.now() < end) {
        await frame();
        const now = busy() ? "" : sig();
        same = now && now === prev ? same + 1 : 0;
        if (same >= 2) return;
        prev = now;
      }
    }, left)
    .catch(() => {});
}
