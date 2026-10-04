// feat-overview-10 (backup calendar): the pure day-grouping and week-grid
// logic, driven without a browser. fix-177 adds the per-stack progress and
// merge reducers the same way: no DOM, no network.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  calendarDays,
  calendarInputs,
  calendarNoBackup,
  calendarProgress,
  calendarWeeks,
  dayDetail,
  earliestSnapshot,
  monthDays,
  shiftMonth,
  withStackResult,
} from "../js/backupcalendar.js";
// Fixed dates only: the calendar is driven by the dates it is given.
import { at } from "./support/clock.js";
void at;

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

// ── fix-177: per-stack progress and merge ────────────────────────────────

test("withStackResult folds one outcome in without touching the rest", () => {
  /** @type {Record<string, import("../js/backupcalendar.js").StackResult>} */
  const a = { gateway: { status: "pending" } };
  const b = withStackResult(a, "media", { status: "pending" });
  assert.deepEqual(
    a,
    { gateway: { status: "pending" } },
    "the input is untouched",
  );
  assert.deepEqual(b, {
    gateway: { status: "pending" },
    media: { status: "pending" },
  });
});

test("calendarProgress: nothing settled yet is 0%, not done", () => {
  const p = calendarProgress({
    gateway: { status: "pending" },
    media: { status: "pending" },
  });
  assert.deepEqual(p, { total: 2, loaded: 0, pct: 0, done: false, failed: [] });
});

test("calendarProgress: ok, empty and failed all count as loaded", () => {
  const p = calendarProgress({
    gateway: { status: "ok", times: [1] },
    media: { status: "empty" },
    backups: {
      status: "failed",
      reason: "no answer from the host within 170 s",
    },
    jellyfin: { status: "pending" },
  });
  assert.equal(p.total, 4);
  assert.equal(p.loaded, 3);
  assert.equal(p.pct, 75);
  assert.equal(p.done, false);
  assert.deepEqual(p.failed, [
    { stack: "backups", reason: "no answer from the host within 170 s" },
  ]);
});

test("calendarProgress: every stack settled is done, and failures sort by name", () => {
  const p = calendarProgress({
    zulu: { status: "failed", reason: "timeout" },
    alpha: { status: "failed", reason: "host down" },
  });
  assert.equal(p.done, true);
  assert.deepEqual(
    p.failed.map((f) => f.stack),
    ["alpha", "zulu"],
  );
});

test("calendarProgress of no stacks at all is 0%, not done (nothing to wait for)", () => {
  assert.deepEqual(calendarProgress({}), {
    total: 0,
    loaded: 0,
    pct: 0,
    done: false,
    failed: [],
  });
});

test("calendarInputs: only ok stacks count toward expected, times carried through", () => {
  const { stacks, expected } = calendarInputs({
    gateway: { status: "ok", times: [1, 2] },
    media: { status: "ok", times: [] },
    backups: { status: "failed", reason: "timeout" },
    syncthing: { status: "empty" },
    jellyfin: { status: "pending" },
  });
  assert.deepEqual(stacks, { gateway: [1, 2], media: [] });
  assert.deepEqual(expected, ["gateway", "media"]);
});

// ── fix-202: a stack with nothing to keep is named at once, never pending ──

test("calendarNoBackup: only no_backup stacks are listed, sorted", () => {
  const names = calendarNoBackup({
    syncthing: { status: "no_backup" },
    gateway: { status: "ok", times: [] },
    almanac: { status: "no_backup" },
    backups: { status: "failed", reason: "timeout" },
  });
  assert.deepEqual(names, ["almanac", "syncthing"]);
});

test("calendarNoBackup of nothing (or nothing excluded) is empty", () => {
  assert.deepEqual(calendarNoBackup({}), []);
  assert.deepEqual(
    calendarNoBackup({ gateway: { status: "ok", times: [] } }),
    [],
  );
});

test("calendarProgress: a no_backup stack counts as loaded, never as failed", () => {
  const p = calendarProgress({
    gateway: { status: "ok", times: [1] },
    syncthing: { status: "no_backup" },
    jellyfin: { status: "pending" },
  });
  assert.equal(p.total, 3);
  assert.equal(p.loaded, 2);
  assert.deepEqual(p.failed, []);
});

test("calendarInputs: a no_backup stack never counts toward expected", () => {
  const { stacks, expected } = calendarInputs({
    gateway: { status: "ok", times: [1] },
    syncthing: { status: "no_backup" },
  });
  assert.deepEqual(stacks, { gateway: [1] });
  assert.deepEqual(expected, ["gateway"]);
});

test("calendarInputs: a no_backup stack never drags a night's ratio down", () => {
  const { stacks, expected } = calendarInputs({
    gateway: { status: "ok", times: [SUN_20] },
    syncthing: { status: "no_backup" },
  });
  const days = calendarDays(stacks, expected, 1, SUN_20, UTC);
  assert.equal(days[0].tone, "ok");
  assert.deepEqual(days[0].expected, ["gateway"]);
});

test("calendarInputs: a failed stack never drags a night's ratio down", () => {
  const SUN_20 = Date.UTC(2026, 8, 20, 12) / 1000;
  const { stacks, expected } = calendarInputs({
    gateway: { status: "ok", times: [SUN_20] },
    backups: {
      status: "failed",
      reason: "no answer from the host within 170 s",
    },
  });
  const days = calendarDays(stacks, expected, 1, SUN_20, UTC);
  // Only "gateway" was trustworthy, and it backed up: ok, not warn or bad —
  // "backups" not reading is not the same as "backups" not backing up.
  assert.equal(days[0].tone, "ok");
  assert.deepEqual(days[0].expected, ["gateway"]);
});

// ── fix-214: a real month, not a fixed 35-day strip ─────────────────────

test("monthDays: September 2026 has exactly 30 days, oldest first", () => {
  const days = monthDays({}, [], 2026, 9, "2026-09-30", null, UTC);
  assert.equal(days.length, 30);
  assert.equal(days[0].date, "2026-09-01");
  assert.equal(days[29].date, "2026-09-30");
});

test("monthDays: February in a leap year has 29 days", () => {
  const days = monthDays({}, [], 2028, 2, "2028-02-29", null, UTC);
  assert.equal(days.length, 29);
});

test("monthDays: a day after today is the future, never a false bad/red", () => {
  const days = monthDays(
    { gateway: [] },
    ["gateway"],
    2026,
    9,
    "2026-09-20",
    null,
    UTC,
  );
  assert.equal(days[19].date, "2026-09-20");
  assert.equal(days[19].tone, "bad", "today itself is still verdicted");
  assert.equal(days[20].tone, "future");
  assert.equal(days[29].tone, "future");
});

test("monthDays: a day before the fleet's oldest snapshot ever is 'before', not 'bad'", () => {
  const sep20 = Date.UTC(2026, 8, 20, 12) / 1000;
  const days = monthDays(
    { gateway: [] },
    ["gateway"],
    2026,
    9,
    "2026-09-30",
    sep20,
    UTC,
  );
  assert.equal(days[18].date, "2026-09-19");
  assert.equal(days[18].tone, "before");
  assert.equal(days[19].date, "2026-09-20");
  assert.equal(days[19].tone, "bad", "the day it started is a real verdict");
});

test("monthDays: a day that did back up reads ok, same rule as calendarDays", () => {
  const sep20 = Date.UTC(2026, 8, 20, 12) / 1000;
  const days = monthDays(
    { gateway: [sep20] },
    ["gateway"],
    2026,
    9,
    "2026-09-30",
    sep20,
    UTC,
  );
  assert.equal(days[19].tone, "ok");
});

test("monthDays feeds calendarWeeks the same way calendarDays does: never more than 6 weeks", () => {
  for (let m = 1; m <= 12; m++) {
    const days = monthDays({}, [], 2026, m, "2026-12-31", null, UTC);
    const weeks = calendarWeeks(days);
    assert.ok(
      weeks.length <= 6,
      `month ${m} produced ${weeks.length} week rows`,
    );
    for (const w of weeks) assert.equal(w.length, 7);
  }
});

test("earliestSnapshot: the oldest time across every stack, or null for nothing read yet", () => {
  assert.equal(earliestSnapshot({ gateway: [300, 100], media: [200] }), 100);
  assert.equal(earliestSnapshot({}), null);
  assert.equal(earliestSnapshot({ gateway: [] }), null);
});

test("shiftMonth: a month back from January lands on last December", () => {
  assert.deepEqual(shiftMonth({ year: 2026, month: 1 }, -1), {
    year: 2025,
    month: 12,
  });
});

test("shiftMonth: a month forward from December lands on next January", () => {
  assert.deepEqual(shiftMonth({ year: 2026, month: 12 }, 1), {
    year: 2027,
    month: 1,
  });
});

test("shiftMonth: a year (12 months) forward or back lands on the same month", () => {
  assert.deepEqual(shiftMonth({ year: 2026, month: 5 }, 12), {
    year: 2027,
    month: 5,
  });
  assert.deepEqual(shiftMonth({ year: 2026, month: 5 }, -12), {
    year: 2025,
    month: 5,
  });
});

// ── fix-214: the day detail panel, built from the same `results` the grid
// reads — no fetch of its own ─────────────────────────────────────────────

test("dayDetail: a stack that backed up that night names its own snapshot time(s)", () => {
  const sep20 = Date.UTC(2026, 8, 20, 12) / 1000;
  const rows = dayDetail(
    { gateway: { status: "ok", times: [sep20] } },
    "2026-09-20",
    UTC,
  );
  assert.deepEqual(rows, [
    { stack: "gateway", state: "backed_up", times: [sep20] },
  ]);
});

test("dayDetail: a stack read ok but with no snapshot that night is 'missing'", () => {
  const rows = dayDetail(
    { gateway: { status: "ok", times: [] } },
    "2026-09-20",
    UTC,
  );
  assert.deepEqual(rows, [{ stack: "gateway", state: "missing", times: [] }]);
});

test("dayDetail: no_backup and not-yet-read states are their own, never folded into 'missing'", () => {
  const rows = dayDetail(
    {
      syncthing: { status: "no_backup" },
      jellyfin: { status: "pending" },
      backups: { status: "failed", reason: "timeout" },
    },
    "2026-09-20",
    UTC,
  );
  assert.deepEqual(rows, [
    { stack: "backups", state: "not_read", times: [] },
    { stack: "jellyfin", state: "not_read", times: [] },
    { stack: "syncthing", state: "no_backup", times: [] },
  ]);
});
