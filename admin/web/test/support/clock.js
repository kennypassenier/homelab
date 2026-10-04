// redesign-final (B, coordinator 2026-10-04): the unit tests' one clock. A
// unit test never reads the real clock: it names a fixed moment through
// `at` (and may then name dates around it, scripts/check-test-clock.mjs
// allows that in a file that imports this), and measures a duration with
// `since` (performance.now, no date).

/**
 * A fixed moment, unix seconds.
 * @param {string} iso
 */
export const at = (iso) => Date.parse(iso) / 1000;

/**
 * Milliseconds since `t0` (a `performance.now()` reading).
 * @param {number} t0
 */
export const since = (t0) => performance.now() - t0;
