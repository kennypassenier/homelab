// redesign-final (Kenny, 2026-10-04): the whole-screen findings parked for
// 3.71.1, read from parked.json: a case by its name, or a class of the one
// layout walk (redesign-final-45; the walk asks `parkedClass`). A parked
// class passes only while it still has findings. `parkedAware(test)` wraps node:test's
// `test`: a case on the list passes only while it is still red, and fails
// the run the moment it passes ("take it off the parked list"), so a
// parked case can neither hide a pass nor stay parked after its fix. Every
// case not on the list runs as it is. scripts/invariants-run.sh and the
// release gate (.githooks/gate-carry.sh) print the list on every run.
import { readFileSync } from "node:fs";

/** @typedef {{name?: string, class?: string, finding: string, date: string, note: string}} Parked */

/** @type {Parked[]} */
export const PARKED = JSON.parse(
  readFileSync(new URL("./parked.json", import.meta.url), "utf8"),
).parked;

/** @param {string} name */
export const parkedFor = (name) => PARKED.find((p) => p.name === name);

/** A class of the layout walk Kenny parked. @param {string} cls */
export const parkedClass = (cls) => PARKED.find((p) => p.class === cls);

/**
 * The verdict on a parked case's run: null while it is still red (its
 * error), else the error that fails the run.
 * @param {Parked} p
 * @param {unknown} err what the case's body threw, undefined when it passed
 * @returns {Error | null}
 */
export const parkedVerdict = (p, err) =>
  err === undefined
    ? new Error(
        `parked case passes now: take it off admin/web/test-e2e/parked.json and docs/INVARIANTS.md (${p.finding}; ${p.note})`,
      )
    : null;

/**
 * @template {(...a: any[]) => any} T
 * @param {T} test node:test's test
 * @returns {T}
 */
export function parkedAware(test) {
  return /** @type {T} */ (
    (/** @type {string} */ name, /** @type {any[]} */ ...rest) => {
      const p = parkedFor(name);
      if (!p) return test(name, ...rest);
      const fn = rest.pop();
      return test(
        name,
        ...rest,
        async (/** @type {any} */ t, /** @type {any[]} */ ...more) => {
          let err;
          try {
            await fn(t, ...more);
          } catch (e) {
            err = e;
          }
          const fail = parkedVerdict(p, err);
          if (fail) throw fail;
          t.diagnostic?.(
            `parked (${p.date}, ${p.note}): still red: ${String(/** @type {any} */ (err)?.message ?? err).split("\n")[0]}`,
          );
        },
      );
    }
  );
}

/** The list as the runner and the gate print it. */
export const parkedLines = () => [
  `invariants: ${PARKED.length} finding(s) parked for 3.71.1 (admin/web/test-e2e/parked.json; a parked case or layout class that passes fails the run):`,
  ...PARKED.map(
    (p) =>
      `  - ${p.name ?? `layout class ${p.class}`}\n      ${p.finding} — ${p.note}`,
  ),
];
