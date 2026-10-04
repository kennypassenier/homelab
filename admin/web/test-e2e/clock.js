// redesign-final (B, coordinator 2026-10-04): the whole-screen run's one
// clock. scripts/invariants-run.sh injects one moment (INVARIANTS_CLOCK,
// unix seconds; HOMELAB_ADMIN_DEMO_CLOCK on the demo host), and the demo
// host, every browser context (harness.js installs it) and the cases all
// count from it, so no case depends on the time of day it runs at (two
// Activity cases went red after midnight UTC). Durations are measured with
// performance.now(), which is no date. scripts/check-test-clock.mjs refuses
// any other clock in a test.

/** The injected moment, unix ms (a fixed default when run alone). */
const CLOCK0 = Number(process.env.INVARIANTS_CLOCK ?? "1791028800") * 1000;
/** When this process started counting from it. */
const T0 = performance.now();

/** Now on the injected clock, unix ms. */
export const clockNow = () => Math.round(CLOCK0 + (performance.now() - T0));

/** Now on the injected clock, unix seconds. */
export const clockNowS = () => Math.floor(clockNow() / 1000);
