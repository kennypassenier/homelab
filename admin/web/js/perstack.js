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
