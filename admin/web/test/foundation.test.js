// feat-shell-5/6 (redesign 3.71.0): the pure halves of the shared
// building blocks — the per-stack identity mark, the sparkline, and the
// time chart's maths (DESIGN_LANGUAGE.md §10, §11).
import { test } from "node:test";
import assert from "node:assert/strict";
import { markCells, nameHash, sparkPath } from "../js/ui.js";
import {
  fmtValue,
  hourChange,
  indexAt,
  niceMax,
  toggleSource,
} from "../js/timechart.js";
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

test("feat-shell-5: a sparkline spans its box, lowest at the bottom", () => {
  const { line, area } = sparkPath([1, 3, 2], 100, 28);
  assert.equal(line, "M0.0,26.0L50.0,2.0L100.0,14.0");
  assert.ok(area.startsWith(line) && area.endsWith("L100,28L0,28Z"));
  assert.equal(sparkPath([5, 5], 100, 28).line, "M0.0,26.0L100.0,26.0");
});

test("feat-shell-6: a legend click turns one source on or off; all on is none", () => {
  let sel = new Set();
  sel = toggleSource(sel, 1, 3);
  assert.deepEqual([...sel], [1]);
  sel = toggleSource(sel, 2, 3);
  assert.deepEqual([...sel].sort(), [1, 2]);
  sel = toggleSource(sel, 0, 3);
  assert.equal(sel.size, 0, "every source picked is the same as none");
  sel = toggleSource(toggleSource(new Set(), 1, 3), 1, 3);
  assert.equal(sel.size, 0);
});

test("feat-shell-6: the tooltip's reading, its change over the hour, and the axis", () => {
  /** @type {[number, number][]} */
  const pts = [0, 1, 2, 3, 4].map((i) => [i * 1800, i * 10]);
  assert.equal(indexAt(pts, 2600), 1);
  assert.equal(indexAt(pts, 99999), 4);
  // Half-hour points: an hour back is two points.
  assert.equal(hourChange(pts, 4), 20);
  assert.equal(hourChange(pts, 1), 10);
  assert.equal(niceMax(87, "percent"), 100);
  assert.equal(niceMax(9, "percent"), 10);
  assert.equal(niceMax(1.3), 1.5);
  assert.equal(fmtValue(42.5, "percent"), "43%");
  assert.equal(fmtValue(2048, "bytes"), "2.0 KiB");
  assert.equal(fmtValue(1536, "rate"), "1.5 KiB/s");
  assert.equal(fmtValue(12345), "12.3k");
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
