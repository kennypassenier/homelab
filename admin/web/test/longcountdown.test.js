// gap 124: the Update flow's 7-day undo window reads in days.
import { test } from "node:test";
import assert from "node:assert/strict";

import { DAY_MS, daysLeft } from "../js/longcountdown.js";

test("gap 124: a window of more than a day reads as days and minutes, rounded down", () => {
  assert.equal(daysLeft(7 * DAY_MS), "7 d 00:00");
  assert.equal(daysLeft(7 * DAY_MS - 1000), "6 d 23:59");
  assert.equal(daysLeft(DAY_MS + 5 * 60_000), "1 d 00:05");
  assert.equal(daysLeft(2 * DAY_MS + 13 * 3_600_000 + 7 * 60_000), "2 d 13:07");
  assert.equal(daysLeft(-5), "0 d 00:00");
});
