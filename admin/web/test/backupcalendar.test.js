// feat-overview-10 (backup calendar): the pure day-grouping and week-grid
// logic, driven without a browser. fix-177 adds the per-stack progress and
// merge reducers the same way: no DOM, no network.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  calendarDays,
  calendarInputs,
  calendarProgress,
  calendarWeeks,
  withStackResult,
} from "../js/backupcalendar.js";

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
