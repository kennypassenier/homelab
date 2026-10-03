// redesign-integrate-8 (coordinator, 2026-10-03): three sweep runs in a row
// printed counts that did not add up ("pressed 194 of 275" with four
// refusals listed): the sweep counted "catalog minus failed" as pressed,
// so a control it never pressed (left out, or dropped) counted as passed.
// Every control now has one outcome, and a run whose counts do not sum
// fails.
import { test } from "node:test";
import assert from "node:assert/strict";
import { catalogDiff, sweepAccount } from "../test-e2e/sweepkey.js";

test("redesign-integrate-8: the sweep accounts for every catalog control once: passed, failed, conditional or not pressed", () => {
  const ids = ["a", "b", "c", "d"];
  const all = new Map(
    /** @type {[string, any][]} */ ([
      ["a", "passed"],
      ["b", "failed"],
      ["c", "conditional"],
      ["d", "skipped"],
    ]),
  );
  const acct = sweepAccount(ids, all);
  assert.equal(
    acct.line,
    "1 of 4 passed, 1 failed, 1 conditional, 1 not pressed",
  );
  assert.deepEqual(acct.skipped, ["d"]);
  // An entry dropped silently (no outcome) fails the run, naming it.
  const dropped = new Map(all);
  dropped.delete("c");
  assert.throws(() => sweepAccount(ids, dropped), /no outcome: c/);
  // An outcome for a control the catalog does not have fails too.
  assert.throws(
    () => sweepAccount(ids, new Map([...all, ["e", "passed"]])),
    /outcome of no control: e/,
  );
  // So does an id the catalog holds twice.
  assert.throws(() => sweepAccount([...ids, "a"], all), /twice: a/);
});

test("redesign-integrate-8: a change in the catalog's size since the stamp is named by the controls added and removed", () => {
  const key = (/** @type {string} */ id) => JSON.stringify([id, "p"]);
  const d = catalogDiff(["a", "b", "c"].map(key), ["a", "c", "x", "y"]);
  assert.deepEqual(d, { before: 3, now: 4, added: ["x", "y"], removed: ["b"] });
});
