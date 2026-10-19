// redesign-schedules (release 3.71.0): the Schedules page's pure half — the
// host's wall clock, the next runs a schedule makes, the sentence the
// drawer reads as, the week the calendar draws and the words every cell
// uses. The demo Kenny approved on 2026-10-03 is the reference for every
// string below.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  busyModel,
  cadenceWords,
  countText,
  coverageModel,
  dateTimeText,
  dayStart,
  draftBody,
  draftFor,
  historyModel,
  isoDate,
  movedTime,
  nextRuns,
  nextUp,
  runsWithClash,
  scheduleTitle,
  seriesOf,
  shortWhen,
  slotPeriods,
  TEMPLATES,
  weekEvents,
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

test("redesign-schedules: cadence, titles and counters in the demo's words", () => {
  assert.equal(
    cadenceWords({ every: "day", at: "10:00" }),
    "every day at 10:00",
  );
  assert.equal(
    cadenceWords({ every: "week", days: [3, 0], at: "22:00" }),
    "on Mon, Thu at 22:00",
  );
  assert.equal(
    cadenceWords({ every: "week", days: [0, 1, 2, 3, 4], at: "18:00" }),
    "on weekdays at 18:00",
  );
  assert.equal(
    cadenceWords({ every: "week", days: [5, 6], at: "09:00" }),
    "at the weekend at 09:00",
  );
  assert.equal(
    cadenceWords({ every: "once", date: "2026-10-05", at: "08:00" }),
    "once on 05/10/2026 at 08:00",
  );
  assert.equal(
    shortWhen({ every: "week", days: [5, 6], at: "09:00" }),
    "Weekend 09:00",
  );
  assert.equal(shortWhen({ every: "day", at: "03:30" }), "Daily 03:30");
  assert.equal(scheduleTitle("Back up", "films"), "Back up · films");
  assert.equal(scheduleTitle("Patch the fleet", "_host"), "Patch the fleet");
  assert.equal(
    scheduleTitle("ZFS replicate", "_host"),
    "ZFS replicate · the whole host",
  );
  const six = [1, 2, 3, 4, 5, 6].map((i) =>
    view(`s${i}`, { every: "day", at: "10:00" }, { enabled: i !== 6 }),
  );
  assert.equal(countText(six), "6 schedules · 5 on");
  assert.equal(countText(six.slice(5)), "1 schedule · 0 on");
});

test("redesign-schedules: the form's defaults, the templates, dates and the body it sends", () => {
  const ctx = {
    stacks: ["films", "notes"],
    hostTarget: "_host",
    now: NOW,
    zone: Z,
    nightly: 2,
  };
  const d = draftFor(null, null, ctx);
  assert.deepEqual(
    [d.action, d.stack, d.every, d.at],
    ["backup", "films", "day", "03:00"],
  );
  assert.equal(TEMPLATES.length, 4);
  const t = draftFor(null, TEMPLATES[2], ctx);
  assert.deepEqual([t.every, t.date, t.at], ["once", "2026-10-04", "20:00"]);
  assert.equal(isoDate("05/10/2026"), "2026-10-05");
  assert.equal(isoDate("2026-10-05"), "2026-10-05");
  assert.equal(isoDate("31/02/2026"), "");
  assert.equal(isoDate("soon"), "");
  assert.equal(draftBody({ ...d, every: "week", days: [] }).ok, false);
  const ok = draftBody({ ...d, note: " n " });
  assert.ok(ok.ok && ok.body.note === "n");
  assert.deepEqual(movedTime("03:30"), { at: "03:45", later: true });
  assert.deepEqual(movedTime("23:50"), { at: "23:35", later: false });
});

test("redesign-schedules: the week gives every run, the round as reference, the next run ringed and a clash named", () => {
  const next = nextRuns({ every: "day", at: "10:00" }, NOW, 1, Z)[0];
  const a = view(
    "a",
    { every: "day", at: "10:00" },
    { stack: "films" },
    { next_run: next },
  );
  const b = view(
    "b",
    { every: "day", at: "03:00" },
    { stack: "notes" },
    { next_run: next },
  );
  const off = view("c", { every: "day", at: "11:00" }, { enabled: false });
  const ev = weekEvents([a, b, off], {
    now: NOW,
    zone: Z,
    nightly: 3,
    label: (x) => x,
    hostTarget: "_host",
  });
  assert.ok(
    ev.filter((e) => e.state === "reference").length >= 70,
    "a round a day",
  );
  assert.equal(ev.filter((e) => e.state === "next").length, 1);
  assert.ok(
    ev.every(
      (e) =>
        e.id.startsWith("c@") === (e.state === "off") ||
        e.state === "reference",
    ),
  );
  const clash = ev.find(
    (e) =>
      e.id.startsWith("b@") &&
      /same minute as the host's nightly round/.test(e.detail),
  );
  assert.ok(clash, "03:00 starts with the round");
  assert.equal(seriesOf("backup"), 1);
  assert.equal(seriesOf("patch"), 3);
  assert.equal(seriesOf("guards"), 5);
});

test("redesign-schedules: the busy hours, the clash lines and the preview's conflicts", () => {
  const a = view("a", { every: "day", at: "03:30" }, { stack: "films" });
  const b = view(
    "b",
    { every: "week", days: [0, 3], at: "03:30" },
    { stack: "notes" },
  );
  const m = busyModel([a, b], {
    now: NOW,
    zone: Z,
    nightly: 3,
    name: (s) => s.stack,
  });
  assert.equal(m.grid.length, 7);
  assert.equal(m.clashes.length, 1, "one line for the pair on Mon and Thu");
  assert.deepEqual(m.clashes[0].days, [0, 3]);
  assert.equal(m.clashes[0].time, "03:30");
  const runs = runsWithClash([a], { every: "day", at: "03:30" }, NOW, 5, {
    zone: Z,
    nightly: 3,
    name: (s) => s.stack,
  });
  assert.equal(runs.length, 5);
  assert.ok(runs.every((r) => r.conflict === "Same minute as films"));
  const none = runsWithClash([a], { every: "day", at: "05:00" }, NOW, 2, {
    zone: Z,
    nightly: 3,
    name: (s) => s.stack,
  });
  assert.ok(none.every((r) => !r.conflict));
  assert.equal(dayStart(NOW, Z), zonedInstant(2026, 10, 3, 0, 0, Z));
});

test("redesign-schedules: coverage names the stacks without a backup schedule", () => {
  const v = [
    view("a", { every: "day", at: "03:00" }, { stack: "films" }),
    view(
      "b",
      { every: "day", at: "04:00" },
      { stack: "notes", action: "update" },
    ),
    view("c", { every: "day", at: "05:00" }, { stack: "off", enabled: false }),
  ];
  const c = coverageModel(v, ["films", "notes", "off"]);
  assert.deepEqual(c.gaps, ["notes", "off"]);
  assert.ok(c.has("notes", "update"));
  assert.equal(nextUp(v), null, "no next run known");
});

test("redesign-schedules: the last fourteen slots are ran, failed, missed or no record, padded at the start", () => {
  const when = /** @type {const} */ ({ every: "day", at: "10:00" });
  const slots = nextRuns(when, NOW - 5 * 86400, 5, Z);
  const v = view("a", when, { last_missed: { slot: slots[1], why: "down" } });
  const job = (
    /** @type {number} */ n,
    /** @type {number} */ slot,
    state = "done",
  ) =>
    /** @type {any} */ ({
      job: n,
      state,
      origin: { from: "schedule", schedule: "a", slot },
      queued_at: slot,
      started_at: slot,
      finished_at: slot + 60,
      action: "backup",
      stack: "films",
    });
  const p = slotPeriods(v, [job(1, slots[0]), job(2, slots[2], "failed")], {
    now: NOW,
    zone: Z,
  });
  assert.equal(p.length, 14);
  const named = p.filter(
    (x) => x.state !== "none" || !x.key.startsWith("none-"),
  );
  const states = named.slice(-8).map((x) => x.state);
  assert.ok(states.includes("up") && states.includes("down"));
  assert.equal(p.find((x) => x.key === String(slots[2]))?.state, "down");
  assert.match(
    p.find((x) => x.key === String(slots[1]))?.tip ?? "",
    /Missed .*dashboard was down/,
  );
  assert.equal(p.find((x) => x.key === String(slots[0]))?.href, "/jobs?job=1");
});

test("redesign-schedules: the history draws the schedules' jobs as bars, the busiest lanes, the missed slot as a mark", () => {
  const when = /** @type {const} */ ({ every: "day", at: "10:00" });
  const a = view("a", when, {
    stack: "films",
    last_missed: { slot: NOW - 86400, why: "busy" },
  });
  const b = view("b", when, { stack: "notes" });
  const job = (
    /** @type {number} */ n,
    /** @type {string} */ sch,
    /** @type {string} */ state,
  ) =>
    /** @type {any} */ ({
      job: n,
      state,
      origin: { from: "schedule", schedule: sch, slot: NOW - 3600 * n },
      queued_at: NOW - 3600 * n,
      started_at: NOW - 3600 * n,
      finished_at: NOW - 3600 * n + 60,
      action: "backup",
      stack: "films",
    });
  const manual = /** @type {any} */ ({
    job: 9,
    state: "done",
    origin: { from: "manual" },
    queued_at: NOW,
    started_at: NOW,
    action: "backup",
    stack: "films",
  });
  const m = historyModel(
    [a, b],
    [job(1, "a", "done"), job(2, "a", "failed"), job(3, "b", "done"), manual],
    { now: NOW, zone: Z, label: (x) => x },
  );
  assert.equal(m.runs.length, 3, "a manual job is no schedule's run");
  assert.deepEqual(
    m.lanes.map((l) => l.id),
    ["a", "b"],
  );
  assert.equal(m.runs.find((r) => r.id === "j2")?.state, "failed");
  assert.equal(m.points.length, 1);
  assert.match(m.points[0].detail, /Missed .*previous run was still going/);
  assert.equal(m.hidden, 0);
  void dateTimeText;
  void zoneParts;
});
