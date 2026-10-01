// feat-overview-10 (backup calendar): the pure day-grouping and week-grid
// logic, driven without a browser.
import { test } from "node:test";
import assert from "node:assert/strict";
import { calendarDays, calendarWeeks } from "../js/backupcalendar.js";

const UTC = "UTC";
// 2026-09-20 is a Sunday (UTC midday, so a local-midnight day boundary
// never puts it a day off for any real timezone).
const SUN_20 = Date.UTC(2026, 8, 20, 12) / 1000;

test("every expected stack present that day reads ok", () => {
  const days = calendarDays(
    { gateway: [SUN_20], media: [SUN_20] },
    ["gateway", "media"],
    1,
    SUN_20,
    UTC,
  );
  assert.equal(days.length, 1);
  assert.equal(days[0].date, "2026-09-20");
  assert.equal(days[0].tone, "ok");
  assert.deepEqual(days[0].backed_up, ["gateway", "media"]);
  assert.deepEqual(days[0].missing, []);
});

test("some but not every expected stack reads warn, with the missing ones named", () => {
  const days = calendarDays(
    { gateway: [SUN_20] },
    ["gateway", "media"],
    1,
    SUN_20,
    UTC,
  );
  assert.equal(days[0].tone, "warn");
  assert.deepEqual(days[0].missing, ["media"]);
  assert.equal(days[0].ratio, 0.5);
});

test("no expected stack present that night reads bad", () => {
  const days = calendarDays({}, ["gateway"], 1, SUN_20, UTC);
  assert.equal(days[0].tone, "bad");
});

test("a day with no expected stacks at all (an empty fleet) is muted, not bad", () => {
  const days = calendarDays({}, [], 1, SUN_20, UTC);
  assert.equal(days[0].tone, "muted");
});

test("two snapshots the same stack the same night count once", () => {
  const days = calendarDays(
    { gateway: [SUN_20, SUN_20 + 3600] },
    ["gateway"],
    1,
    SUN_20,
    UTC,
  );
  assert.deepEqual(days[0].backed_up, ["gateway"]);
});

test("the window is oldest first and spans exactly `days` days", () => {
  const days = calendarDays({}, [], 3, SUN_20, UTC);
  assert.equal(days.length, 3);
  assert.equal(days[0].date, "2026-09-18");
  assert.equal(days[2].date, "2026-09-20");
});

test("calendarWeeks pads the first and last week to 7 cells, Monday first", () => {
  // Sunday 2026-09-20 alone: weekday 6 (Mon=0), so 6 leading nulls then
  // the one real day, then one trailing week is not started.
  const days = calendarDays({}, [], 1, SUN_20, UTC);
  const weeks = calendarWeeks(days);
  assert.equal(weeks.length, 1);
  assert.equal(weeks[0].length, 7);
  assert.equal(weeks[0][6]?.date, "2026-09-20");
  assert.deepEqual(weeks[0].slice(0, 6), [null, null, null, null, null, null]);
});

test("calendarWeeks of an empty day list is an empty grid", () => {
  assert.deepEqual(calendarWeeks([]), []);
});
