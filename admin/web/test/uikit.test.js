// redesign-kit-1 (3.71.0): the pure halves of the building blocks the
// Backups, Host, Schedules and Secrets pages brought into the shared
// ui.js — the KPI meter, the segmented toggle group, the sortable table,
// the hover card and row menu placement, the toast's replace rule and the
// stack colours.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  chartColour,
  hueColour,
  menuPlace,
  meterParts,
  sortValue,
  tipPlace,
  toastsToDrop,
  toggleValue,
} from "../js/ui.js";

test("redesign-kit-1: a KPI meter is clamped to its bar, with its tick and tone", () => {
  assert.deepEqual(meterParts({ pct: 41 }), { fill: 41, mark: null, tone: "" });
  assert.deepEqual(meterParts({ pct: 140, mark: -5, tone: "bad" }), {
    fill: 100,
    mark: 0,
    tone: "bad",
  });
  // An unknown reading draws an empty track, never a broken width.
  assert.deepEqual(meterParts({ pct: null }), {
    fill: 0,
    mark: null,
    tone: "",
  });
  assert.equal(meterParts({ pct: Number.NaN }).fill, 0);
  assert.equal(meterParts(null).fill, 0);
  assert.equal(meterParts({ pct: 50, mark: 46 }).mark, 46);
});

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

test("redesign-kit-1: a sortable header's cells sort numbers as numbers", () => {
  assert.equal(sortValue("120"), 120);
  assert.equal(sortValue("Gateway"), "gateway");
  assert.equal(sortValue(""), "");
  assert.ok(Number(sortValue("9")) < Number(sortValue("10")));
});

test("redesign-kit-1: the hover card sits under its element, above it at the foot, inside the screen", () => {
  const view = { w: 1000, h: 800 };
  assert.deepEqual(
    tipPlace({ left: 400, top: 100, bottom: 120, width: 20 }, 200, 80, view),
    { left: 310, top: 128 },
  );
  // No room below: above the element.
  assert.deepEqual(
    tipPlace({ left: 400, top: 700, bottom: 720, width: 20 }, 200, 80, view),
    { left: 310, top: 612 },
  );
  // Never past the left or right edge.
  assert.equal(
    tipPlace({ left: 0, top: 0, bottom: 10, width: 10 }, 200, 50, view).left,
    8,
  );
  assert.equal(
    tipPlace({ left: 990, top: 0, bottom: 10, width: 10 }, 200, 50, view).left,
    792,
  );
});

test("redesign-kit-1: a row menu opens under its button's right edge, upwards near the foot", () => {
  const view = { w: 1200, h: 800, x: 0, y: 300 };
  assert.deepEqual(
    menuPlace({ top: 100, bottom: 130, right: 1000 }, 150, view, 260),
    { top: 434, left: 740 },
  );
  assert.deepEqual(
    menuPlace({ top: 700, bottom: 730, right: 1000 }, 150, view, 260),
    { top: 846, left: 740 },
  );
  assert.equal(
    menuPlace({ top: 100, bottom: 130, right: 100 }, 150, view, 260).left,
    8,
    "never past the left edge",
  );
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
