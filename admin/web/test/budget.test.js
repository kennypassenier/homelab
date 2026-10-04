// redesign-final-51 (Kenny, 2026-10-04: "Dit soort ballooning van tests mag
// niet meer voorkomen"): the whole-screen suite's budget guard.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  GROWTH,
  measureOf,
  overBudget,
  raiseFaults,
  readBudget,
} from "../test-e2e/budget.js";

const B = { seconds: 1000, cases: 100 };

test("redesign-final-51: a run fits up to 20 % over its budget and fails beyond, on time or on cases", () => {
  assert.equal(GROWTH, 0.2);
  assert.deepEqual(overBudget(B, { seconds: 1200, cases: 120 }), []);
  assert.equal(overBudget(B, { seconds: 1201, cases: 100 }).length, 1);
  assert.match(overBudget(B, { seconds: 1201, cases: 100 })[0], /1201 s/);
  assert.equal(overBudget(B, { seconds: 900, cases: 121 }).length, 1);
  assert.match(overBudget(B, { seconds: 900, cases: 121 })[0], /121 cases/);
  assert.equal(overBudget(B, { seconds: 1300, cases: 130 }).length, 2);
  // A smaller suite always fits.
  assert.deepEqual(overBudget(B, { seconds: 10, cases: 1 }), []);
});

test("redesign-final-51: raising the budget needs a new reason line; lowering it does not", () => {
  const before = {
    ...B,
    reason: "measured on the pruned suite, 2026-10-04",
    measured: "x",
  };
  assert.deepEqual(raiseFaults(before, { ...before, seconds: 900 }), []);
  assert.match(
    raiseFaults(before, { ...before, seconds: 1100 }).join(),
    /without a new reason/,
  );
  assert.match(
    raiseFaults(before, { ...before, cases: 101 }).join(),
    /without a new reason/,
  );
  assert.deepEqual(
    raiseFaults(before, {
      ...before,
      cases: 110,
      reason: "ten new cases for the 3.72 backups page, Kenny's go 2026-10-04",
    }),
    [],
  );
  assert.match(raiseFaults(null, { ...before, reason: "x" }).join(), /reason/);
  assert.match(
    raiseFaults(null, { ...before, seconds: 0 }).join(),
    /positive number/,
  );
});

test("redesign-final-51: a run's cases and duration come from node's own summary", () => {
  const tap =
    "TAP version 13\nok 1 - a\n# tests 3\n# pass 3\n# duration_ms 5\n";
  assert.deepEqual(measureOf(tap, 61.4), { seconds: 61, cases: 3 });
  assert.deepEqual(measureOf("", 5), { seconds: 5, cases: 0 });
});

test("redesign-final-51: the committed budget is set and says why", () => {
  const b = readBudget();
  assert.deepEqual(raiseFaults(null, b), []);
  assert.match(b.measured, /^\d{4}-\d{2}-\d{2}/);
});
