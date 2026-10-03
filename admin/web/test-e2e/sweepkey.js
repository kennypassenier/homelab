// redesign-integrate-8: one control's catalog entry as the Live view sweep
// pressed it — what it is, where it lives and how it is reached; its words
// (`what`) left out, a reworded description does not move a control. The
// sweep (invariants.e2e.js) stamps these into sweep-stamp.json when it
// passes; admin/web/test/drivecatalog.test.js refuses a catalog control
// whose key is not stamped.
import { createHash } from "node:crypto";

/**
 * A value with every object's keys in order: the dashboard serves the
 * catalog through serde (keys sorted), the declarations keep their own
 * order; one control is one key either way.
 * @param {any} v
 * @returns {any}
 */
const canon = (v) =>
  Array.isArray(v)
    ? v.map(canon)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.keys(v)
            .sort()
            .map((k) => [k, canon(v[k])]),
        )
      : v;

/** @param {any} c a catalog control (js/drivecatalog.json `controls`) */
export const sweepKey = (c) =>
  JSON.stringify(
    canon([
      c.id,
      c.page,
      c.opens,
      c.row ?? null,
      c.href ?? null,
      c.was ?? [],
      c.shows ?? null,
      c.reach ?? [],
      !!c.twins,
    ]),
  );

/**
 * The hash of the catalog a sweep ran against: its controls' keys, sorted.
 * The sweep presses every control or writes no stamp, so a stamp's
 * `controls` are exactly that catalog's keys and must hash to its
 * `catalog`; a key added or moved by hand breaks the match.
 * @param {string[]} keys sweepKey of every control, sorted
 */
export const catalogHash = (keys) =>
  createHash("sha256").update(keys.join("\n")).digest("hex");

/** @typedef {"passed" | "failed" | "conditional" | "skipped"} Outcome */

/**
 * Every catalog control's outcome in one sweep run, accounted for: each
 * id exactly once as passed, failed, conditional (refused naming the state
 * it shows in, which the sweep did not draw) or skipped (left out by
 * INVARIANTS_SWEEP_ONLY). Throws when a control has no outcome, an outcome
 * names no control, an id is in the catalog twice, or the four do not sum
 * to the catalog's size: a run never drops an entry silently.
 * @param {string[]} catalog every control id, in the catalog's order
 * @param {Map<string, Outcome>} outcome
 */
export function sweepAccount(catalog, outcome) {
  /** @type {Record<Outcome, string[]>} */
  const by = { passed: [], failed: [], conditional: [], skipped: [] };
  for (const id of catalog) {
    const o = outcome.get(id);
    if (o) by[o].push(id);
  }
  const missing = catalog.filter((id) => !outcome.has(id));
  const known = new Set(catalog);
  const unknown = [...outcome.keys()].filter((id) => !known.has(id));
  const twice = catalog.filter((id, i) => catalog.indexOf(id) !== i);
  const sum =
    by.passed.length +
    by.failed.length +
    by.conditional.length +
    by.skipped.length;
  if (
    missing.length ||
    unknown.length ||
    twice.length ||
    sum !== catalog.length
  )
    throw new Error(
      `the sweep's counts do not add up: ${catalog.length} controls, ${sum} accounted for; no outcome: ${missing.join(", ") || "none"}; outcome of no control: ${unknown.join(", ") || "none"}; in the catalog twice: ${twice.join(", ") || "none"}`,
    );
  return {
    ...by,
    total: catalog.length,
    line: `${by.passed.length} of ${catalog.length} passed, ${by.failed.length} failed, ${by.conditional.length} conditional, ${by.skipped.length} not pressed`,
  };
}

/**
 * The controls added and removed since the last passing sweep's stamp, by
 * id (a moved control is in neither: its key changed, its id did not).
 * Throws unless they explain the change in the catalog's size.
 * @param {string[]} stampKeys the stamp's `controls`
 * @param {string[]} ids the catalog's ids now
 */
export function catalogDiff(stampKeys, ids) {
  const before = new Set(stampKeys.map((k) => String(JSON.parse(k)[0])));
  const now = new Set(ids);
  const added = ids.filter((id) => !before.has(id));
  const removed = [...before].filter((id) => !now.has(id));
  if (before.size + added.length - removed.length !== now.size)
    throw new Error(
      `the catalog went from ${before.size} to ${now.size} controls, which ${added.length} added and ${removed.length} removed do not explain`,
    );
  return { before: before.size, now: now.size, added, removed };
}
