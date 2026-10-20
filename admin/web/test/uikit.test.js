// redesign-kit-1 (3.71.0): the pure halves of the building blocks the
// Backups, Host, Schedules and Secrets pages brought into the shared
// ui.js — the segmented toggle group, the toast's replace rule and the
// stack colours.
import { test } from "node:test";
import assert from "node:assert/strict";
import { chartColour, hueColour, toastsToDrop, toggleValue } from "../js/ui.js";

test("redesign-kit-1: a toggle group click turns one value on or off; every value on is All", () => {
  let on = new Set();
  on = toggleValue(on, "running", 2);
  assert.deepEqual([...on], ["running"]);
  on = toggleValue(on, "running", 2);
  assert.equal(on.size, 0, "a second click turns it off again");
  on = toggleValue(toggleValue(new Set(), "running", 3), "stopped", 3);
  assert.deepEqual([...on].sort(), ["running", "stopped"]);
  on = toggleValue(on, "paused", 3);
  assert.equal(on.size, 0, "all three on is the same as none");
  const before = new Set(["a"]);
  toggleValue(before, "b", 3);
  assert.deepEqual([...before], ["a"], "the old set is left alone");
});

test("redesign-kit-1: a new toast replaces the plain ones, never one still offering Undo", () => {
  const shown = [{ action: false }, { action: true }, { action: false }];
  assert.deepEqual(toastsToDrop(shown, "plain"), [0, 2]);
  assert.deepEqual(toastsToDrop(shown, "all"), [0, 1, 2]);
  assert.deepEqual(toastsToDrop([], "plain"), []);
});

test("redesign-kit-1: a stack's colour is stable: its topology hue, or its place in the sorted fleet", () => {
  assert.equal(
    hueColour(210),
    "light-dark(hsl(210 65% 42%), hsl(210 70% 64%))",
  );
  const all = ["notes", "admin", "gateway"];
  assert.equal(chartColour("admin", all), "var(--chart-1)");
  assert.equal(chartColour("notes", all), "var(--chart-3)");
  assert.equal(
    chartColour("f", ["a", "b", "c", "d", "e", "f"]),
    "var(--chart-1)",
    "the sixth stack wraps to the first colour",
  );
  assert.deepEqual(
    all,
    ["notes", "admin", "gateway"],
    "the list is not sorted in place",
  );
});
