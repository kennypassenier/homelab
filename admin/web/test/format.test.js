// ui-units: durations and moments in human units, in the viewer's locale.
import { test } from "node:test";
import assert from "node:assert/strict";
import { formatTime, humanDuration } from "../js/format.js";
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

test("a moment reads in the viewer's locale, never as ISO", () => {
  const at = 1790000000; // 2026-09-21 14:13:20 UTC
  const en = formatTime(at, { locale: "en-GB", timeZone: "UTC" });
  assert.match(en, /21 Sept? 2026/);
  assert.match(en, /14:13/);
  const nl = formatTime(at, { locale: "nl-NL", timeZone: "Europe/Brussels" });
  assert.match(nl, /21 sep/);
  assert.match(nl, /16:13/);
  assert.doesNotMatch(en, /T\d\d:/);
  assert.equal(formatTime(null), "—");
  assert.equal(formatTime(0), "—");
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
