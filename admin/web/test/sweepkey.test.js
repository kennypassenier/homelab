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

// redesign-final (coordinator, 2026-10-04): the commit-time stamp guard asks
// a sweep only of the controls whose catalog entries changed against HEAD
// (added, moved, given a `was`); the release gate still asks a full stamp.
test("redesign-final: the controls a commit changed are the catalog's diff against HEAD: added, changed and newly aliased ids", async () => {
  const { changedControls } = await import("../test-e2e/sweepkey.js");
  const c = (/** @type {string} */ id, /** @type {any} */ more = {}) => ({
    id,
    page: "p",
    opens: "view",
    what: "w",
    ...more,
  });
  const head = [c("a"), c("b"), c("c"), c("gone")];
  const now = [
    c("a", { what: "reworded only" }),
    c("b", { row: "x|y" }),
    c("c", { was: [{ id: "old-c" }] }),
    c("new"),
  ];
  assert.deepEqual(changedControls(head, now), ["b", "c", "new"]);
  assert.deepEqual(changedControls(now, now), []);
  // No HEAD catalog (a first commit): every control changed.
  assert.deepEqual(changedControls(null, now), ["a", "b", "c", "new"]);
});

test("redesign-final: a stamp covers a commit when every changed control's entry was pressed; a full stamp covers the whole catalog", async () => {
  const { sweepKey, catalogHash, uncovered, fullStamp, partialStamp } =
    await import("../test-e2e/sweepkey.js");
  const c = (/** @type {string} */ id, /** @type {any} */ more = {}) => ({
    id,
    page: "p",
    opens: "view",
    what: "w",
    ...more,
  });
  const head = [c("a"), c("b")];
  const now = [c("a"), c("b", { row: "1|2" }), c("n")];
  // A partial stamp of the two changed controls covers the commit.
  const part = partialStamp([now[1], now[2]].map(sweepKey), "x");
  assert.equal(part.scope, "partial");
  assert.equal(part.catalog, catalogHash([...part.controls].sort()));
  assert.deepEqual(uncovered(part, head, now), []);
  // Missing one: named.
  const one = partialStamp([sweepKey(now[2])], "x");
  assert.deepEqual(uncovered(one, head, now), ["b"]);
  // An old full stamp of HEAD's catalog covers nothing that changed.
  const old = fullStamp(head.map(sweepKey), "x");
  assert.deepEqual(uncovered(old, head, now), ["b", "n"]);
  // The gate: only a full stamp of this very catalog will do.
  assert.equal(fullStamp(now.map(sweepKey), "x").scope, "full");
  const { gateRefusal } = await import("../test-e2e/sweepkey.js");
  assert.equal(gateRefusal(fullStamp(now.map(sweepKey), "x"), now), null);
  assert.match(String(gateRefusal(part, now)), /partial/);
  assert.match(String(gateRefusal(old, now)), /not pressed: b, n/);
});
