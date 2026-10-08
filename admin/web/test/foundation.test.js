// feat-shell-5/6 (redesign 3.71.0): the pure halves of the shared
// building blocks — the per-stack identity mark (DESIGN_LANGUAGE.md §10,
// §11). The time chart's own maths (toggleSource, indexAt, hourChange,
// niceMax, fmtValue) moved to kp-themes' chart.js with the chart itself
// (js/charts.js) and is kp's own to test.
import { test } from "node:test";
import assert from "node:assert/strict";
import { markCells, nameHash } from "../js/ui.js";
import { AREAS, moreAreas, phoneTabs } from "../js/areas.js";

test("feat-shell-5: a stack's mark is deterministic, mirrored, and differs between stacks", () => {
  const a = markCells("gateway");
  assert.deepEqual(markCells("gateway"), a);
  assert.equal(a.hue, nameHash("gateway") % 360);
  for (const [x, y] of a.cells) {
    assert.ok(x >= 0 && x < 5 && y >= 0 && y < 5);
    // Mirrored around the middle column.
    assert.ok(a.cells.some(([x2, y2]) => x2 === 4 - x && y2 === y));
  }
  const b = markCells("kp-soft");
  assert.notDeepEqual(
    [a.hue, a.cells],
    [b.hue, b.cells],
    "two stacks look alike",
  );
});

test("feat-shell-1: the phone shows the four frequent areas as tabs and the rare two under More", () => {
  assert.deepEqual(
    AREAS.map((a) => a.label),
    ["Apps", "Inbox", "Stacks", "Activity", "Backups", "System"],
  );
  assert.deepEqual(
    phoneTabs().map((a) => a.label),
    ["Apps", "Inbox", "Stacks", "Activity"],
  );
  assert.deepEqual(
    moreAreas().map((a) => a.label),
    ["Backups", "System"],
  );
});
