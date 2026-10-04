// redesign-final-51 (Kenny, 2026-10-04: "Dit soort ballooning van tests mag
// niet meer voorkomen", after one night spent 6 h 16 min in whole-screen
// runs): the whole-screen suite has a budget, test-e2e/budget.json: its
// duration and its case count as the release gate measured them. A full
// run (scripts/invariants-run.sh without INVARIANTS_ONLY, which the gate
// makes) fails when either grows more than 20 % over it. Raising the budget
// is a commit that edits the file and says why in a new `reason` line
// (.githooks/pre-commit refuses a raise without one).
import { readFileSync } from "node:fs";

/** How far a run may outgrow its budget before the gate fails. */
export const GROWTH = 0.2;

/** @typedef {{seconds: number, cases: number}} Measure */
/** @typedef {Measure & {reason: string, measured: string}} Budget */

/** @returns {Budget} */
export const readBudget = () =>
  JSON.parse(readFileSync(new URL("./budget.json", import.meta.url), "utf8"));

/**
 * What a run did beyond its budget; empty when it fits.
 * @param {Measure} budget
 * @param {Measure} run
 * @returns {string[]}
 */
export function overBudget(budget, run) {
  /** @type {string[]} */
  const out = [];
  const limit = (/** @type {number} */ n) => n * (1 + GROWTH);
  if (run.seconds > limit(budget.seconds))
    out.push(
      `the whole-screen suite took ${run.seconds} s, more than ${Math.round(GROWTH * 100)} % over its budget of ${budget.seconds} s`,
    );
  if (run.cases > limit(budget.cases))
    out.push(
      `the whole-screen suite has ${run.cases} cases, more than ${Math.round(GROWTH * 100)} % over its budget of ${budget.cases}`,
    );
  return out;
}

/**
 * A commit's change of the budget: a raise (more seconds or more cases)
 * needs a new reason. Empty when the change is allowed.
 * @param {Budget | null} before HEAD's budget (null: the file is new)
 * @param {Budget} after the staged one
 * @returns {string[]}
 */
export function raiseFaults(before, after) {
  /** @type {string[]} */
  const out = [];
  for (const k of /** @type {const} */ (["seconds", "cases"]))
    if (!(Number.isFinite(after[k]) && after[k] > 0))
      out.push(`budget.json: ${k} is no positive number`);
  if (!after.reason || after.reason.trim().length < 20)
    out.push("budget.json: the reason line says why in a sentence");
  if (!before) return out;
  const raised = after.seconds > before.seconds || after.cases > before.cases;
  if (raised && after.reason.trim() === before.reason.trim())
    out.push(
      `budget.json raises the budget (${before.seconds} s / ${before.cases} cases to ${after.seconds} s / ${after.cases} cases) without a new reason line`,
    );
  return out;
}

/**
 * The cases and the duration of a run, from its TAP progress file (node's
 * own summary lines at the end).
 * @param {string} tap
 * @param {number} seconds the run's wall time
 * @returns {Measure}
 */
export function measureOf(tap, seconds) {
  const m = [...tap.matchAll(/^# tests (\d+)$/gm)].pop();
  return { seconds: Math.round(seconds), cases: m ? Number(m[1]) : 0 };
}
