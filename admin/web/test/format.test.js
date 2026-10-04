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

test("redesign-final X4: every moment reads dd/mm/yyyy HH:MM, 24-hour, in Europe/Brussels (Kenny's rule, fix-216)", () => {
  const at = 1790000000; // Mon 2026-09-21 14:13:20 UTC
  assert.equal(formatDateTime(at), "21/09/2026 16:13", "Brussels by default");
  assert.equal(formatDateTime(at, { timeZone: "UTC" }), "21/09/2026 14:13");
  assert.equal(formatDay(at), "21/09/2026");
  assert.equal(formatClock(at), "16:13");
  assert.equal(formatClock(at, { seconds: true }), "16:13:20");
  // Midnight is 00:00, never 24:00; another year is the same shape.
  assert.equal(
    formatDateTime(Date.UTC(2025, 11, 29, 23, 5) / 1000),
    "30/12/2025 00:05",
  );
  // Never a weekday or a month's name ("Sat 3 Oct", "30 Sep 12:14").
  assert.doesNotMatch(
    formatDateTime(at),
    /Mon|Tue|Wed|Thu|Fri|Sat|Sun|Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec/,
  );
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
