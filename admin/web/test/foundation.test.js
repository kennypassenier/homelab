// feat-shell-5/6 (redesign 3.71.0): the pure halves of the shared
// building blocks. The time chart's own maths (toggleSource, indexAt, hourChange,
// niceMax, fmtValue) moved to kp-themes' chart.js with the chart itself
// (js/charts.js) and is kp's own to test.
import { test } from "node:test";
import assert from "node:assert/strict";
import { AREAS, moreAreas, phoneTabs } from "../js/areas.js";

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
