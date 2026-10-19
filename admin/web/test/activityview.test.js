// redesign-activity (3.71.0, the approved Activity demo): the view models
// the Activity page draws from — who started each operation, the History
// feed's filter and day groups, the open incidents of the attention band,
// the KPI strip in exact numbers, and a running job's step strip.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actorOf,
  byDay,
  dayStart,
  feedRows,
  filterFromParams,
  filterToParams,
  heatModel,
  inRange,
  incidentFor,
  kpis,
  openFailures,
  outcomeWord,
  ownerOf,
  showFilters,
  stackOf,
  stackWord,
  timelineModel,
  stepSegments,
  verbOf,
  windowDays,
} from "../js/activityview.js";

const STACKS = ["gateway", "kp-soft", "notes"];
const NOW = 1_790_000_000;

/** @returns {import("../js/activity.js").OpEntry} */
const op = (/** @type {Partial<import("../js/activity.js").OpEntry>} */ o) => ({
  kind: "op",
  start: NOW - 3600,
  end: NOW - 3500,
  label: "deploy",
  subject: "deploy-gateway",
  req: 7,
  by: "admin",
  ok: true,
  steps: [],
  ...o,
});

test("redesign-activity: History names who started an operation", () => {
  assert.deepEqual(actorOf(op({ by: "Claude (Live view)" })), {
    text: "Claude (Live view)",
    kind: "claude",
  });
  assert.equal(
    actorOf(op({ req: null, by: null, label: "scheduled-backup" })).kind,
    "nightly",
  );
  assert.equal(actorOf(op({ by: "admin" })).text, "Kenny");
  assert.equal(actorOf(op({ by: "Kenny" })).text, "Kenny");
  assert.equal(actorOf(op({ req: null, by: null })).text, "the host");
  assert.deepEqual(
    actorOf({ kind: "phase", start: 1, end: 2, name: "backup", count: 6 }),
    { text: "the host", kind: "nightly" },
  );
  // The dashboard's own job knows its origin better than the host's line.
  /** @type {Map<number, import("../js/jobs.js").Origin>} */
  const origins = new Map([
    [7, { from: "claude", by: "live view" }],
    [8, { from: "schedule", schedule: "weekly patch", slot: 1 }],
  ]);
  assert.equal(actorOf(op({ req: 7 }), origins).kind, "claude");
  assert.deepEqual(actorOf(op({ req: 8 }), origins), {
    text: "schedule “weekly patch”",
    kind: "schedule",
  });
});

test("redesign-activity: an operation reads as a verb and a stack", () => {
  assert.equal(verbOf("scheduled-backup"), "Back up");
  assert.equal(verbOf("disable"), "Park");
  assert.equal(verbOf("guards"), "Add log guards");
  assert.equal(verbOf("self-test"), "Self test");
  assert.equal(stackOf("backup-kp-soft", STACKS), "kp-soft");
  assert.equal(stackOf("deploy gateway", STACKS), "gateway");
  assert.equal(stackOf("patch-fleet", STACKS), null);
  const [r] = feedRows([op({ subject: "backup-notes", label: "backup" })], {
    stacks: STACKS,
  });
  assert.equal(r.what, "Back up notes");
  assert.equal(r.stack, "notes");
  assert.equal(r.took, 100);
});

test("kp11-activity: an old ?show= reaches the table as its own filters, the timeline's range narrows rows", () => {
  const rows = feedRows(
    [
      op({ start: NOW - 10, end: NOW - 5, ok: false, error: "boom" }),
      op({ start: NOW - 20, end: NOW - 15, by: "Claude (Live view)" }),
      op({
        start: NOW - 30,
        end: NOW - 25,
        req: null,
        by: null,
        label: "scheduled-backup",
        subject: "backup-notes",
      }),
      op({ start: NOW - 40, end: NOW - 35 }),
    ],
    { stacks: STACKS },
  );
  assert.deepEqual(showFilters(rows, new Set()), { outcome: [], by: [] });
  assert.deepEqual(showFilters(rows, new Set(["failed"])).outcome, ["Failed"]);
  assert.deepEqual(showFilters(rows, new Set(["claude"])).by, [
    "Claude (Live view)",
  ]);
  assert.deepEqual(showFilters(rows, new Set(["nightly"])).by, [
    "nightly round",
  ]);
  assert.deepEqual(
    rows.map((r) => outcomeWord(r)),
    ["Failed", "Done", "Done", "Done"],
  );
  assert.equal(rows.filter((r) => inRange(r, null)).length, 4);
  assert.equal(
    rows.filter((r) => inRange(r, { from: NOW - 25, to: NOW })).length,
    2,
  );
  assert.equal(stackWord(rows[0]), rows[0].stack ?? "host");
});

test("redesign-activity: the filter lives in the address", () => {
  const f = filterFromParams(
    new URLSearchParams("show=bogus,claude,failed&q=notes&from=10&to=20"),
  );
  // One of All / Failed / Nightly / By Claude (the demo's one-of switch):
  // an old address naming several keeps the first known one.
  assert.deepEqual([...f.show], ["claude"]);
  assert.deepEqual(f.range, { from: 10, to: 20 });
  assert.deepEqual(filterToParams(f), {
    show: "claude",
    q: "notes",
    from: "10",
    to: "20",
  });
  assert.deepEqual(filterToParams({ show: new Set(), q: " ", range: null }), {
    show: null,
    q: null,
    from: null,
    to: null,
  });
  assert.equal(windowDays(new URLSearchParams("days=7")), 7);
  assert.equal(windowDays(new URLSearchParams("days=9")), 14);
});

test("redesign-activity: History groups by day with exact counts", () => {
  const today = dayStart(NOW);
  const rows = feedRows(
    [
      op({ start: today + 60, end: today + 90 }),
      op({ start: today + 30, end: today + 50, ok: false }),
      op({ start: today - 3600, end: today - 3500 }),
      {
        kind: "phase",
        start: today - 7200,
        end: today - 6000,
        name: "backup",
        count: 3,
      },
    ],
    { stacks: STACKS },
  );
  const days = byDay(rows);
  assert.equal(days.length, 2);
  assert.deepEqual(
    days.map((d) => [d.day, d.ops, d.failed]),
    [
      [today, 2, 1],
      [dayStart(today - 3600), 1, 0],
    ],
  );
});

test("redesign-activity: a failure stays open until the same operation succeeds, and finds its incident bundle", () => {
  const rows = feedRows(
    [
      op({
        start: NOW - 100,
        end: NOW - 90,
        ok: false,
        error: "locked",
        label: "backup",
        subject: "backup-notes",
      }),
      op({
        start: NOW - 9000,
        end: NOW - 8990,
        ok: false,
        label: "deploy",
        subject: "deploy-gateway",
      }),
      op({
        start: NOW - 5000,
        end: NOW - 4990,
        ok: true,
        label: "deploy",
        subject: "deploy-gateway",
      }),
    ],
    { stacks: STACKS },
  );
  const open = openFailures(rows);
  assert.deepEqual(
    open.map((r) => r.what),
    ["Back up notes"],
  );
  assert.equal(
    incidentFor(open[0], [
      `${NOW - 90}-backup-notes`,
      `${NOW - 90}-deploy-gateway`,
      "junk",
    ]),
    `${NOW - 90}-backup-notes`,
  );
  assert.equal(incidentFor(open[0], [`${NOW - 90 - 7200}-backup-notes`]), null);
});

test("redesign-activity: the KPI strip shows exact numbers", () => {
  const today = dayStart(NOW);
  const rows = feedRows(
    [
      ...Array.from({ length: 11 }, (_, i) =>
        op({ start: today + 4000 + i, end: today + 4010 + i }),
      ),
      op({
        start: today + 3000,
        end: today + 3010,
        ok: false,
        req: null,
        by: null,
        label: "scheduled-backup",
        subject: "backup-notes",
      }),
      op({
        start: today + 2000,
        end: today + 2010,
        req: null,
        by: null,
        label: "scheduled-backup",
        subject: "backup-kp-soft",
      }),
      {
        kind: "phase",
        start: today + 1900,
        end: today + 3100,
        name: "backup",
        count: 2,
      },
    ],
    { stacks: STACKS },
  );
  const k = kpis({
    rows,
    days: 14,
    now: today + 5000,
    open: openFailures(rows),
    running: [],
    runningText: "",
  });
  const by = Object.fromEntries(k.map((x) => [x.key, x]));
  assert.equal(by.ops.value, "13", "never abbreviated, never capped");
  assert.equal(by.ok.value, "92.3");
  assert.equal(by.ok.ctx, "1 failed in 14 days");
  assert.equal(by.incidents.value, "1");
  assert.equal(by.incidents.tone, "bad");
  assert.match(by.nightly.ctx, /1 of 2 backed up/);
  assert.equal(by.nightly.tone, "warn");
  assert.equal(by.running.value, "0");
  assert.equal(by.ops.spark?.length, 14);
});

test("redesign-activity: a running job's steps strip", () => {
  /** @type {any} */
  const p = { n: 3, m: 5, finished: false, step: "recreate" };
  assert.deepEqual(
    stepSegments(p).map((s) => s.state),
    ["done", "done", "run", "todo", "todo"],
  );
  assert.deepEqual(
    stepSegments({ ...p, m: 40 }),
    [],
    "too many: a bar instead",
  );
  assert.deepEqual(stepSegments(null), []);
});

// Senior review finding 8: the "By" column names a person, never a raw
// token: a workstation's token is Kenny on that machine's CLI or TUI.
test("redesign-activity: a token reads as its owner", () => {
  assert.equal(ownerOf("admin"), "Kenny");
  assert.equal(ownerOf("wsl"), "Kenny · CLI on wsl");
  assert.equal(ownerOf("garuda-pc"), "Kenny · CLI on garuda-pc");
  assert.equal(ownerOf("Kenny"), "Kenny");
  assert.equal(ownerOf("Claude (Live view)"), "Claude (Live view)");
  assert.deepEqual(actorOf(op({ by: "wsl" })), {
    text: "Kenny · CLI on wsl",
    kind: "person",
  });
});

test("kp11-activity: the timeline draws the nightly round once, a schedule's day as one bar, a failure as a mark", () => {
  const today = dayStart(NOW);
  const rows = feedRows(
    [
      {
        kind: "phase",
        start: today + 100,
        end: today + 600,
        name: "backup",
        count: 2,
      },
      op({
        start: today + 120,
        end: today + 200,
        req: null,
        by: null,
        label: "scheduled-backup",
        subject: "backup-notes",
        ok: false,
        error: "boom",
      }),
      op({
        start: today + 300,
        end: today + 320,
        req: null,
        by: "schedule tuesday",
        label: "scheduled-update",
        subject: "update-notes",
      }),
      op({ start: today + 400, end: today + 410 }),
    ],
    { stacks: STACKS },
  );
  const m = timelineModel(rows, { from: today - 86400, now: today + 1000 });
  assert.deepEqual(
    m.runs.map((r) => [r.lane, r.series, r.state]).sort(),
    [
      ["nightly", 1, "ok"],
      ["nightly", 2, "ok"],
      ["ops", 3, "ok"],
    ],
    "the round's own ops are inside its bar",
  );
  assert.equal(m.points.length, 1);
  assert.equal(m.points[0].lane, "nightly");
  assert.equal(m.rounds.size, 1);
});

test("kp11-activity: the stack-by-day grid counts per stack and day, and the fleet's failed share", () => {
  const today = dayStart(NOW);
  const rows = feedRows(
    [
      op({ start: today + 60, end: today + 90, subject: "update-notes" }),
      op({
        start: today + 30,
        end: today + 50,
        ok: false,
        subject: "update-notes",
      }),
      op({
        start: today - 86400 + 60,
        end: today - 86400 + 90,
        subject: "update-gateway",
      }),
    ],
    { stacks: STACKS },
  );
  const m = heatModel(rows, { stacks: STACKS, days: 3, now: NOW });
  assert.equal(m.columns.length, 3);
  const last = m.columns[2].key;
  assert.deepEqual(
    m.cell("notes", last) && [
      m.cell("notes", last)?.n,
      m.cell("notes", last)?.f,
    ],
    [2, 1],
  );
  assert.equal(m.cell("kp-soft", last), null, "not touched");
  assert.deepEqual(m.fleet(last), { ran: 1, bad: 1 });
});
