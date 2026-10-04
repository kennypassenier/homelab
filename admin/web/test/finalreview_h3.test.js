// redesign-final (3.71.0's final whole-dashboard review, 2026-10-04), H3:
// FLOWS.md §5 approves a stepped Restore flow page (flows/restore.html);
// the dashboard had a picker dialog leading to a second dialog.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  restoreHref,
  restoreNights,
  restoreStep,
  whatWillHappen,
} from "../js/restoreflow.js";

const H = 3600;
const at = (/** @type {number} */ daysAgo, /** @type {number} */ hour) => {
  const d = new Date(2026, 9, 4, 12, 0);
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, 4, 0, 0);
  return d.getTime() / 1000;
};
const NOW = new Date(2026, 9, 4, 12, 0).getTime() / 1000;

test("redesign-final-h3: the Restore flow has its own address under Backups", () => {
  assert.equal(restoreHref("notes"), "/backups/restore?stack=notes");
  assert.equal(
    restoreHref("notes", "db", "a1b2c3"),
    "/backups/restore?stack=notes&app=db&snapshot=a1b2c3",
  );
  assert.equal(restoreHref(null), "/backups/restore");
});

test("redesign-final-h3: the nights are newest first, a missed night shown and not pickable, today without a snapshot no miss", () => {
  const snaps = [0, 1, 3].map((d, i) => ({
    id: `full${i}`,
    short_id: `s${d}`,
    time: at(d, 3),
  }));
  const n = restoreNights(snaps, NOW, 5);
  assert.deepEqual(
    n.map((x) => [x.missed, x.snap?.short_id ?? null]),
    [
      [false, "s0"],
      [false, "s1"],
      [true, null],
      [false, "s3"],
    ],
  );
  assert.match(n[0].label, /^Today, 03:04$/);
  assert.match(n[2].label, /^[A-Z][a-z]{2} \d{1,2} [A-Z][a-z]{2}$/);
  for (const x of n) assert.doesNotMatch(x.label, /\d+\/\d+\/\d+/);
  const late = restoreNights(
    [{ id: "x", short_id: "y", time: at(1, 3) }],
    NOW,
    3,
  );
  assert.equal(late[0].missed, false, "today is no miss before tonight");
  void H;
});

test("redesign-final-h3: step 3 says what will happen, the safety copy as the undo; the stepper follows the choices", () => {
  const w = whatWillHappen({
    stack: "notes",
    app: "db",
    night: "Fri 2 Oct, 03:04",
    native: false,
  });
  assert.equal(w.length, 4);
  assert.match(w[1], /safety copy .*undo/);
  assert.match(w[2], /Fri 2 Oct, 03:04/);
  assert.equal(restoreStep({ app: null, night: null, running: false }), 1);
  assert.equal(restoreStep({ app: "db", night: null, running: false }), 2);
  assert.equal(restoreStep({ app: "db", night: "s1", running: false }), 3);
  assert.equal(restoreStep({ app: "db", night: "s1", running: true }), 4);
});
