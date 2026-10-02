// Generic per-stack progressive-read bookkeeping, shared by any page that
// asks one route per stack concurrently instead of one all-stacks call: one
// slow or hung stack then only ever holds up its own cell/row, named under
// a progress bar, rather than one timeout losing the whole page (fix-177,
// the backup calendar) or silently vanishing from the table (fix-179, the
// Backups page). Pure, so it is driven by node tests without a DOM or a
// network; no page-specific shape lives here — generic over each page's own
// result type (the calendar's `ok`/`empty`/`times`, the Backups page's
// `ok`/`native`/`repos`), so a page's own `StackResult` union stays its own
// without this module widening or narrowing it.
//
// `pending`: not yet answered. `failed`: the read itself did not finish
// (network, timeout, the host down) — named under the progress bar, never
// folded into a page's own "found nothing" reading. Any other `status` is a
// page's own successful outcome; this module never looks inside one.

/**
 * `results` with one more stack's outcome folded in, without touching the
 * rest.
 * @template {{status: string}} T
 * @param {Record<string, T>} results
 * @param {string} name
 * @param {T} outcome
 * @returns {Record<string, T>}
 */
export function withStackResult(results, name, outcome) {
  return { ...results, [name]: outcome };
}

/**
 * The progress bar's numbers and the failures to name under it. A stack
 * still "pending" counts toward `total` but not `loaded`, so the bar fills
 * as each one settles — ok or failed, either way "read".
 * @template {{status: string}} T
 * @param {Record<string, T>} results
 * @returns {{total: number, loaded: number, pct: number, done: boolean,
 *   failed: {stack: string, reason: string}[]}}
 */
export function stackReadProgress(results) {
  const names = Object.keys(results);
  const total = names.length;
  let loaded = 0;
  /** @type {{stack: string, reason: string}[]} */
  const failed = [];
  for (const name of names) {
    const r = results[name];
    if (r.status === "pending") continue;
    loaded += 1;
    // `r`'s concrete type (a page's own union) carries `reason` on its
    // "failed" member; T's constraint here only promises `status`, so the
    // field is read through `any` rather than widening T to match it.
    if (r.status === "failed")
      failed.push({ stack: name, reason: /** @type {any} */ (r).reason });
  }
  failed.sort((a, b) => a.stack.localeCompare(b.stack));
  return {
    total,
    loaded,
    pct: total ? Math.round((loaded / total) * 100) : 0,
    done: total > 0 && loaded === total,
    failed,
  };
}

/**
 * fix-224 (Kenny, Dutch: "welke doet die dan niet? waarom kan ik dat niet
 * zien?" — the calendar's "Stacks read" bar used to say only a count, never
 * which stack was still outstanding). One row per stack, sorted by name, so
 * a page can paint a chip grid naming every stack's own state instead of a
 * bare fraction: `read` (settled, successful, whatever that means for the
 * page), `no_backup` (fix-202's terminal "keeps nothing by design"),
 * `reading` (still pending, with how long — `startedAt`'s reading for this
 * stack, or `nowMs` itself if the caller never recorded one), and `failed`
 * (the read did not finish, named with the host's own reason — this is also
 * where a stack that outlived its per-stack timeout lands, since the page's
 * own retry loop turns that into a `failed` outcome once it gives up).
 * @template {{status: string}} T
 * @param {Record<string, T>} results
 * @param {Record<string, number>} startedAt unix ms each stack's read began
 * @param {number} nowMs
 * @returns {{stack: string, state: "read"|"reading"|"no_backup"|"failed",
 *   seconds?: number, reason?: string}[]}
 */
export function stackChips(results, startedAt, nowMs) {
  return Object.keys(results)
    .sort()
    .map((stack) => {
      const r = results[stack];
      if (r.status === "pending") {
        const began = startedAt[stack] ?? nowMs;
        return {
          stack,
          state: /** @type {const} */ ("reading"),
          seconds: Math.max(0, Math.round((nowMs - began) / 1000)),
        };
      }
      if (r.status === "no_backup")
        return { stack, state: /** @type {const} */ ("no_backup") };
      if (r.status === "failed")
        return {
          stack,
          state: /** @type {const} */ ("failed"),
          reason: /** @type {any} */ (r).reason,
        };
      return { stack, state: /** @type {const} */ ("read") };
    });
}
