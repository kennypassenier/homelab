// ui-units: durations and moments in human units, in the viewer's locale.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  formatClock,
  formatDateTime,
  formatDay,
  humanDuration,
} from "../js/format.js";
import { sortKeys } from "../js/sortkeys.js";

test("durations step from seconds to minutes, hours and days", () => {
  assert.equal(humanDuration(0), "0 s");
  assert.equal(humanDuration(45), "45 s");
  assert.equal(humanDuration(120), "2 min");
  assert.equal(humanDuration(125), "2 min 5 s");
  assert.equal(humanDuration(3600), "1 h");
  assert.equal(humanDuration(7320), "2 h 2 min");
  assert.equal(humanDuration(86400), "1 day");
  assert.equal(humanDuration(3 * 86400 + 5 * 3600), "3 days 5 h");
  assert.equal(humanDuration(null), "—");
  assert.equal(humanDuration(-1), "—");
});

test("redesign-final X4: every moment reads as the demos write it, never dd/mm/yyyy", () => {
  const at = 1790000000; // Mon 2026-09-21 14:13:20 UTC
  const now = { now: at, timeZone: "UTC" };
  assert.equal(formatDateTime(at, now), "Mon 21 Sep, 14:13");
  assert.equal(
    formatDateTime(at, { now: at, timeZone: "Europe/Brussels" }),
    "Mon 21 Sep, 16:13",
  );
  assert.equal(formatDay(at, now), "Mon 21 Sep");
  assert.equal(formatClock(at, now), "14:13");
  assert.equal(formatClock(at, { ...now, seconds: true }), "14:13:20");
  // Another year is written out.
  const before = Date.UTC(2025, 11, 30, 8, 5) / 1000;
  assert.equal(formatDateTime(before, now), "Tue 30 Dec 2025, 08:05");
  // Midnight is 00:00, never 24:00.
  assert.equal(formatClock(Date.UTC(2026, 9, 3) / 1000, now), "00:00");
  assert.equal(formatDateTime(null), "—");
  assert.equal(formatDateTime(0), "—");
});

test("sort keys compare the number behind a human text", () => {
  const k = sortKeys();
  k.note("duration", "2 min", 120);
  k.note("duration", "45 s", 45);
  k.note("duration", "1 day", 86400);
  /** @type {(a: string, b: string, kind: string, locale: string) => number} */
  const fallback = (a, b) => a.localeCompare(b);
  const cmp = k.compare(fallback);
  const sorted = ["2 min", "1 day", "—", "45 s"].sort((a, b) =>
    cmp(a, b, "duration", "en"),
  );
  assert.deepEqual(sorted, ["—", "45 s", "2 min", "1 day"]);
  // Other kinds go to the table's own comparison untouched.
  assert.equal(cmp("b", "a", "text", "en"), 1);
});
