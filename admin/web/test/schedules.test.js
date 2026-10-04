// redesign-schedules (release 3.71.0): the Schedules page's pure half — the
// host's wall clock, the next runs a schedule makes, the sentence the
// drawer reads as, the week the calendar draws and the words every cell
// uses. The demo Kenny approved on 2026-10-03 is the reference for every
// string below.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  agenda,
  cadenceText,
  countText,
  dateTimeText,
  draftBody,
  draftFor,
  lastRunView,
  nearNightly,
  nextRuns,
  nextUp,
  scheduleTitle,
  sentenceText,
  stackTops,
  TEMPLATES,
  untilText,
  weekPlan,
  zonedInstant,
  zoneParts,
} from "../js/schedules.js";
import { at } from "./support/clock.js";

const Z = "Europe/Brussels";
/** Sat 3 Oct 2026, 07:46 in Brussels (summer time, UTC+2): the demo's now. */
const NOW = at("2026-10-03T05:46:00Z");
const utc = (/** @type {number[]} */ ...a) =>
  Date.UTC(a[0], a[1] - 1, a[2], a[3], a[4]) / 1000;

/**
 * @param {string} id
 * @param {import("../js/schedules.js").When} when
 * @param {Partial<import("../js/schedules.js").Schedule>} [o]
 * @param {Partial<import("../js/schedules.js").ScheduleView>} [v]
 * @returns {import("../js/schedules.js").ScheduleView}
 */
const view = (id, when, o = {}, v = {}) => ({
  schedule: {
    id,
    stack: "films",
    action: "backup",
    args: {},
    when,
    enabled: true,
    note: "",
    created_at: 1,
    handled_until: 1,
    ...o,
  },
  next_run: null,
  next_run_local: null,
  last_job: null,
  ...v,
});
const label = (/** @type {string} */ a) =>
  /** @type {Record<string, string>} */ ({
    backup: "Back up",
    patch: "Patch the fleet",
    "zfs-replicate": "ZFS replicate",
    "deploy-commit": "Deploy a commit",
  })[a] ?? a;

test("redesign-schedules: a host wall-clock time resolves to its instant across both clock changes", () => {
  assert.equal(zonedInstant(2026, 7, 1, 3, 30, Z), utc(2026, 7, 1, 1, 30));
  assert.equal(zonedInstant(2026, 1, 15, 3, 30, Z), utc(2026, 1, 15, 2, 30));
  // Autumn: 02:30 happens twice; the first one counts, as on the host.
  assert.equal(zonedInstant(2026, 10, 25, 2, 30, Z), utc(2026, 10, 25, 0, 30));
  // Spring: 02:30 does not exist; the first minute after the gap (03:00).
  assert.equal(zonedInstant(2026, 3, 29, 2, 30, Z), utc(2026, 3, 29, 1, 0));
  const p = zoneParts(NOW, Z);
  assert.deepEqual(
    [p.y, p.m, p.d, p.hh, p.mm, p.wd],
    [2026, 10, 3, 7, 46, 5],
    "Saturday is weekday 5 (Monday = 0, as the server counts)",
  );
});

test("redesign-schedules: the next runs of a schedule, as the drawer previews them", () => {
  const tueFri = { every: "week", days: [1, 4], at: "02:30" };
  assert.deepEqual(
    nextRuns(/** @type {any} */ (tueFri), NOW, 3, Z).map((t) =>
      dateTimeText(t, Z),
    ),
    ["06/10/2026 02:30", "09/10/2026 02:30", "13/10/2026 02:30"],
  );
  // Later today still counts.
  assert.deepEqual(
    nextRuns({ every: "day", at: "10:00" }, NOW, 2, Z).map((t) =>
      dateTimeText(t, Z),
    ),
    ["03/10/2026 10:00", "04/10/2026 10:00"],
  );
  assert.deepEqual(
    nextRuns({ every: "once", date: "2026-10-05", at: "08:00" }, NOW, 3, Z),
    [utc(2026, 10, 5, 6, 0)],
  );
  assert.deepEqual(
    nextRuns({ every: "once", date: "2026-10-01", at: "08:00" }, NOW, 3, Z),
    [],
    "a one-off in the past has no next run",
  );
  // Across the autumn change a daily 02:30 runs once a day.
  const across = nextRuns(
    { every: "day", at: "02:30" },
    utc(2026, 10, 24, 12, 0),
    3,
    Z,
  );
  assert.deepEqual(
    across.map((t) => dateTimeText(t, Z)),
    ["25/10/2026 02:30", "26/10/2026 02:30", "27/10/2026 02:30"],
  );
});

test("redesign-schedules: when, until, titles and counters in the demo's words", () => {
  assert.equal(
    cadenceText({ every: "day", at: "10:00" }),
    "Every day at 10:00",
  );
  assert.equal(
    cadenceText({ every: "week", days: [3, 0], at: "22:00" }),
    "Mon, Thu at 22:00",
  );
  assert.equal(
    cadenceText({ every: "week", days: [5], at: "14:30" }),
    "Sat at 14:30",
  );
  assert.equal(
    cadenceText({ every: "week", days: [0, 1, 2, 3, 4, 5, 6], at: "01:00" }),
    "Every day at 01:00",
  );
  assert.equal(
    cadenceText({ every: "once", date: "2026-10-05", at: "08:00" }),
    "Once, 05/10/2026 at 08:00",
  );
  assert.equal(untilText(NOW + 2 * 3600 + 14 * 60, NOW), "in 2 h 14 min");
  assert.equal(untilText(NOW + 30 * 60, NOW), "in 30 min");
  assert.equal(untilText(NOW + 3 * 86400 + 600, NOW), "in 3 d");
  // Rounded as a whole: never "in 21 h 60 min" or "in 60 min".
  assert.equal(untilText(NOW + 21 * 3600 + 59 * 60 + 50, NOW), "in 22 h 0 min");
  assert.equal(untilText(NOW + 59 * 60 + 50, NOW), "in 1 h 0 min");
  assert.equal(scheduleTitle("Back up", "films"), "Back up films");
  assert.equal(scheduleTitle("Patch the fleet", "_host"), "Patch the fleet");
  assert.equal(
    scheduleTitle("ZFS replicate", "_host"),
    "ZFS replicate the host",
  );
  assert.equal(
    scheduleTitle("Deploy a commit", "kp-soft"),
    "Deploy a commit kp-soft",
  );
  const six = [1, 2, 3, 4, 5, 6].map((i) =>
    view(`s${i}`, { every: "day", at: "10:00" }, { enabled: i !== 6 }),
  );
  assert.equal(countText(six), "6 schedules · 5 on");
  assert.equal(countText(six.slice(5)), "1 schedule · 0 on");
  assert.equal(nearNightly("02:30", 3), true);
  assert.equal(nearNightly("03:59", 3), true);
  assert.equal(nearNightly("04:00", 3), false);
  assert.equal(nearNightly("02:00", 3), false);
  assert.equal(nearNightly("23:30", 0), true, "an hour before midnight");
  assert.equal(nearNightly("02:30", null), false, "no nightly round");
});

test("redesign-schedules: the drawer's sentence, its defaults, the templates and the body it sends", () => {
  const ctx = {
    stacks: ["films", "notes"],
    hostTarget: "_host",
    now: NOW,
    zone: Z,
  };
  const d = draftFor(null, null, ctx);
  assert.deepEqual(
    [d.action, d.stack, d.every, d.days, d.at],
    ["backup", "films", "week", [1, 4], "02:30"],
    "a new schedule starts as the demo's: Tue and Fri at 02:30",
  );
  assert.equal(
    sentenceText(d, label, "_host"),
    "Run Back up on films on chosen weekdays at 02:30",
  );
  assert.deepEqual(draftBody(d), {
    ok: true,
    body: {
      stack: "films",
      action: "backup",
      args: {},
      when: { every: "week", days: [1, 4], at: "02:30" },
      enabled: true,
      note: "",
    },
  });
  assert.deepEqual(draftBody({ ...d, days: [] }), {
    ok: false,
    field: "days",
    why: "Pick at least one day.",
  });
  assert.equal(draftBody({ ...d, every: "once", date: "" }).ok, false);
  assert.equal(
    sentenceText({ ...d, every: "once", date: "2026-10-05" }, label, "_host"),
    "Run Back up on films once on 05/10/2026 at 02:30",
  );
  const patch = draftFor(null, TEMPLATES[1], ctx);
  assert.deepEqual(
    [patch.action, patch.stack, patch.every, patch.days, patch.at],
    ["patch", "_host", "week", [6], "22:00"],
  );
  assert.equal(
    sentenceText(patch, label, "_host"),
    "Run Patch the fleet on the whole host on chosen weekdays at 22:00",
  );
  assert.equal(TEMPLATES.length, 3);
  const later = draftFor(null, TEMPLATES[2], ctx);
  assert.deepEqual([later.every, later.date], ["once", "2026-10-04"]);
  // Editing keeps what the schedule has, its switch included.
  const v = view(
    "s3",
    { every: "day", at: "06:00" },
    { stack: "_host", action: "zfs-replicate", enabled: false, note: "n" },
  );
  const e = draftFor(v.schedule, null, ctx);
  assert.deepEqual(
    [e.action, e.stack, e.every, e.at, e.enabled, e.note],
    ["zfs-replicate", "_host", "day", "06:00", false, "n"],
  );
});

test("redesign-schedules: the last run names the job, or the slot missed and why", () => {
  const job = (/** @type {string} */ state) =>
    /** @type {any} */ ({ job: 398, state });
  const at = { slot: 100, job: 398 };
  assert.deepEqual(lastRunView(view("a", { every: "day", at: "10:00" }), Z), {
    tone: "none",
    text: "not run yet",
    job: null,
  });
  assert.deepEqual(
    lastRunView(
      view(
        "a",
        { every: "day", at: "10:00" },
        { last_run: at },
        {
          last_job: job("done"),
        },
      ),
      Z,
    ),
    { tone: "ok", text: "ok", job: 398 },
  );
  assert.equal(
    lastRunView(
      view(
        "a",
        { every: "day", at: "10:00" },
        { last_run: at },
        {
          last_job: job("failed"),
        },
      ),
      Z,
    ).tone,
    "bad",
  );
  const sat26 = utc(2026, 9, 26, 12, 30);
  assert.deepEqual(
    lastRunView(
      view(
        "a",
        { every: "day", at: "10:00" },
        {
          last_run: at,
          last_missed: { slot: sat26, why: "down" },
        },
      ),
      Z,
    ),
    { tone: "warn", text: "missed 26/09/2026 (dashboard down)", job: null },
  );
  // A run after the miss is the last run again.
  assert.equal(
    lastRunView(
      view(
        "a",
        { every: "day", at: "10:00" },
        {
          last_run: { slot: sat26 + 86400, job: 9 },
          last_missed: { slot: sat26, why: "busy" },
        },
      ),
      Z,
    ).job,
    9,
  );
});

test("redesign-schedules: the week from today, the nightly round, the now-line, the next run and the agenda", () => {
  const list = [
    view(
      "s1",
      { every: "day", at: "10:00" },
      {},
      {
        next_run: utc(2026, 10, 3, 8, 0),
      },
    ),
    view(
      "s2",
      { every: "week", days: [0, 3], at: "22:00" },
      { stack: "_host", action: "patch" },
      { next_run: utc(2026, 10, 5, 20, 0) },
    ),
    view("s6", { every: "day", at: "12:00" }, { enabled: false }),
  ];
  const w = weekPlan(list, NOW, Z, 3, label);
  assert.deepEqual(
    w.days.map((d) => d.head),
    ["Today", "Sun 4", "Mon 5", "Tue 6", "Wed 7", "Thu 8", "Fri 9"],
  );
  assert.ok(Math.abs(w.nowHour - (7 + 46 / 60)) < 1e-9);
  for (const d of w.days) {
    const host = d.pills.filter((p) => p.host);
    assert.deepEqual(
      host.map((p) => [p.at, p.text]),
      [["03:00", "nightly round"]],
    );
  }
  const today = w.days[0].pills.filter((p) => !p.host);
  assert.deepEqual(
    today.map((p) => [p.at, p.text, p.next, p.off]),
    [
      ["10:00", "Back up films", true, false],
      ["12:00", "Back up films", false, true],
    ],
  );
  assert.deepEqual(
    w.days[2].pills.filter((p) => p.id === "s2").map((p) => p.at),
    ["22:00"],
  );
  assert.equal(nextUp(list)?.schedule.id, "s1");
  const a = agenda(list, NOW, Z, label);
  assert.equal(a.length, 8);
  assert.deepEqual(
    a.slice(0, 3).map((r) => [r.when, r.text]),
    [
      ["03/10/2026 10:00", "Back up films"],
      ["04/10/2026 10:00", "Back up films"],
      ["05/10/2026 10:00", "Back up films"],
    ],
  );
  assert.ok(
    !a.some((r) => r.id === "s6"),
    "a schedule that is off is not listed",
  );
  // A run skipped on purpose shows as skipped, the next one as next.
  const skipped = weekPlan(
    [{ ...list[0], next_run: utc(2026, 10, 4, 8, 0) }],
    NOW,
    Z,
    null,
    label,
  );
  assert.deepEqual(
    skipped.days[0].pills.map((p) => [p.skipped, p.next]),
    [[true, false]],
  );
  assert.deepEqual(
    skipped.days[1].pills.map((p) => p.next),
    [true],
  );
  assert.equal(
    skipped.days[0].pills.filter((p) => p.host).length,
    0,
    "no nightly round when the host has none",
  );
});

test("redesign-schedules: pills that would overlap are stacked, in time order", () => {
  assert.deepEqual(stackTops([60, 20, 62, 200], 20), [60, 20, 80, 200]);
  assert.deepEqual(stackTops([10, 12, 14], 20), [10, 30, 50]);
});
