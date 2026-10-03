// redesign-backups (Backups redesign 3.71): the Backups page's pure logic — which
// night a snapshot belongs to, each heatmap cell's state, the whole-fleet
// row, the KPI numbers, the repository table's groups, filters and sort,
// and the page's address — driven without a browser.
import { test } from "node:test";
import assert from "node:assert/strict";

// Nights are the viewer's own: pin a zone with a daylight-saving change in
// the range the tests walk (Europe/Brussels leaves summer time on
// 2026-10-25).
process.env.TZ = "Europe/Brussels";

const {
  addDays,
  cellState,
  drillState,
  fleetNight,
  humanBytes,
  kpis,
  nightKey,
  nightRange,
  nightsNow,
  offsetFor,
  repoGroups,
  searchFromView,
  stackNight,
  stackWhy,
  ticks,
  viewFromSearch,
} = await import("../js/backupsview.js");
const { nextSort } = await import("../js/sortstate.js");

/** A stack that keeps no data by design. @returns {import("../js/backupsview.js").StackRead} */
const nothingKept = () => ({
  status: "ok",
  noBackup: true,
  times: [],
  native: false,
  repos: [],
});

/** Local wall-clock time in Brussels as unix seconds. */
const at = (/** @type {string} */ iso) => Date.parse(iso) / 1000;

/** @returns {import("../js/backupsview.js").StackRead} */
const ok = (
  /** @type {number[]} */ times,
  /** @type {any[]} */ repos = [],
  native = false,
) => ({ status: "ok", noBackup: false, times, native, repos });

test("a snapshot belongs to the night of its evening: 03:00 counts for the day before, an afternoon for its own day", () => {
  assert.equal(nightKey(at("2026-10-01T03:10:00+02:00")), "2026-09-30");
  assert.equal(nightKey(at("2026-10-01T11:59:00+02:00")), "2026-09-30");
  assert.equal(nightKey(at("2026-10-01T14:00:00+02:00")), "2026-10-01");
});

test("nights step by the calendar, so the end of summer time neither skips nor repeats one", () => {
  assert.equal(addDays("2026-10-24", 1), "2026-10-25");
  assert.equal(addDays("2026-10-26", -1), "2026-10-25");
  assert.equal(addDays("2026-03-01", -1), "2026-02-28");
  const r = nightRange("2026-11-05", 30, 0);
  assert.equal(r.length, 30);
  assert.equal(r[29], "2026-11-05");
  assert.equal(r[0], "2026-10-07");
  assert.equal(new Set(r).size, 30);
  assert.deepEqual(nightRange("2026-11-05", 3, 30), [
    "2026-10-04",
    "2026-10-05",
    "2026-10-06",
  ]);
});

test("last night is over at 06:00; from noon tonight's night is under way", () => {
  assert.deepEqual(nightsNow(at("2026-10-03T07:00:00+02:00")), {
    last: "2026-10-02",
    current: "2026-10-02",
  });
  assert.deepEqual(nightsNow(at("2026-10-03T13:00:00+02:00")), {
    last: "2026-10-02",
    current: "2026-10-03",
  });
  assert.deepEqual(nightsNow(at("2026-10-03T05:00:00+02:00")), {
    last: "2026-10-01",
    current: "2026-10-02",
  });
});

test("a heatmap cell says backed up, missed, before any history, tonight, keeps no data, reading or not read", () => {
  const now = nightsNow(at("2026-10-03T13:00:00+02:00"));
  const s = ok([
    at("2026-09-29T03:00:00+02:00"),
    at("2026-10-01T03:00:00+02:00"),
    at("2026-10-02T03:00:00+02:00"),
  ]);
  assert.equal(cellState(s, "2026-09-28", now), "ok");
  assert.equal(cellState(s, "2026-09-29", now), "miss");
  assert.equal(cellState(s, "2026-09-27", now), "before");
  assert.equal(cellState(s, "2026-10-03", now), "wait");
  assert.equal(cellState(nothingKept(), "2026-09-29", now), "none");
  assert.equal(cellState({ status: "pending" }, "2026-09-29", now), "load");
  assert.equal(
    cellState({ status: "failed", reason: "x" }, "2026-09-29", now),
    "unread",
  );
  // A stack that keeps data but never wrote a snapshot missed every night.
  assert.equal(cellState(ok([]), "2026-09-29", now), "miss");
});

test("the whole-fleet night counts only the stacks expected to back up that night", () => {
  const now = nightsNow(at("2026-10-03T13:00:00+02:00"));
  const stacks = {
    a: ok([at("2026-10-02T03:00:00+02:00")]),
    b: ok([at("2026-09-30T03:00:00+02:00")]),
    c: nothingKept(),
    d: ok([at("2026-10-03T04:00:00+02:00")]),
  };
  const f = fleetNight(stacks, "2026-10-01", now);
  assert.deepEqual(
    { ok: f.ok, expected: f.expected, pct: f.pct },
    { ok: 1, expected: 2, pct: 50 },
  );
  // Before every stack's first snapshot: no expectation at all.
  const early = fleetNight(stacks, "2026-09-01", now);
  assert.equal(early.before, true);
  assert.equal(early.expected, 0);
  assert.equal(fleetNight(stacks, "2026-10-03", now).wait, true);
});

test("the side panel and the hover card name each app's snapshot id that night", () => {
  const now = nightsNow(at("2026-10-03T13:00:00+02:00"));
  const t1 = at("2026-10-02T03:00:00+02:00");
  const t2 = at("2026-10-02T03:30:00+02:00");
  const s = ok(
    [t1, t2],
    [
      { owner: "web", snapshots: [{ id: "a", short_id: "aaaa", time: t1 }] },
      { owner: "db", snapshots: [{ id: "b", short_id: "bbbb", time: t2 }] },
    ],
  );
  const n = stackNight(s, "2026-10-01", now);
  assert.equal(n.state, "ok");
  assert.deepEqual(n.times, [t1, t2]);
  assert.deepEqual(
    n.ids.map((x) => `${x.owner} ${x.short_id}`),
    ["web aaaa", "db bbbb"],
  );
});

test("the KPI numbers are exact: last night's covered stacks by name, the newest snapshot, repositories and drills", () => {
  const now = at("2026-10-03T08:00:00+02:00");
  const night = at("2026-10-03T03:00:00+02:00");
  const stacks = {
    gateway: ok(
      [night, at("2026-10-02T03:00:00+02:00")],
      [
        {
          owner: "traefik",
          snapshot_count: 2,
          snapshots: [],
          newest_snapshot: { id: "x", short_id: "x", time: night },
          drill: { last_attempt: 5, last_pass: 5 },
        },
        {
          owner: "crowdsec",
          snapshot_count: 3,
          snapshots: [],
          newest_snapshot: { id: "y", short_id: "y", time: night + 60 },
          drill: { last_attempt: 9, last_pass: 0, last_error: "0 files" },
        },
      ],
    ),
    notes: ok(
      [at("2026-10-01T03:00:00+02:00")],
      [{ owner: "notes", snapshot_count: 1, snapshots: [] }],
    ),
    old: nothingKept(),
  };
  const k = kpis(stacks, now, 2);
  assert.equal(k.settled, true);
  assert.deepEqual(k.lastNight, {
    night: "2026-10-02",
    covered: 1,
    expected: 2,
    missed: ["notes"],
  });
  assert.deepEqual(k.newest, { time: night + 60, where: "gateway/crowdsec" });
  assert.deepEqual(k.repositories, { count: 3, snapshots: 6, stacks: 2 });
  assert.deepEqual(k.drills, { passed: 1, failed: 1, never: 1, total: 3 });
  assert.equal(k.retired, 2);
  assert.equal(
    kpis({ ...stacks, x: { status: "pending" } }, now, null).settled,
    false,
  );
});

test("a stack without a repository row says why", () => {
  assert.equal(stackWhy(nothingKept()), "keeps no data by design");
  assert.equal(stackWhy(ok([])), "no repository found on the backup target");
  assert.equal(
    stackWhy({ status: "failed", reason: "timed out" }),
    "could not read it: timed out",
  );
  assert.equal(
    stackWhy(ok([], [{ owner: "a", snapshots: [], snapshot_count: 0 }], true)),
    "1 repository · native service",
  );
});

test("the drill state, the seven-night ticks and the size read the repository's own data", () => {
  assert.equal(
    drillState({ owner: "a", snapshots: [], snapshot_count: 0 }),
    "never",
  );
  const r = {
    owner: "a",
    snapshot_count: 2,
    snapshots: [
      { id: "1", short_id: "1", time: at("2026-10-02T03:00:00+02:00") },
      { id: "2", short_id: "2", time: at("2026-09-28T03:00:00+02:00") },
    ],
  };
  assert.deepEqual(ticks(r, "2026-10-01"), [
    false,
    false,
    true,
    false,
    false,
    false,
    true,
  ]);
  assert.equal(humanBytes(null), null);
  assert.equal(humanBytes(48_234_496), "46 MB");
  assert.equal(humanBytes(1536), "1.5 KB");
});

test("a sort header cycles ascending, descending, none; Shift adds a second key", () => {
  let s = /** @type {{key: string, dir: 1 | -1}[]} */ (
    nextSort([], "age", false)
  );
  assert.deepEqual(s, [{ key: "age", dir: 1 }]);
  s = nextSort(s, "age", false);
  assert.deepEqual(s, [{ key: "age", dir: -1 }]);
  assert.deepEqual(nextSort(s, "age", false), []);
  s = nextSort([{ key: "age", dir: 1 }], "size", true);
  assert.deepEqual(s, [
    { key: "age", dir: 1 },
    { key: "size", dir: 1 },
  ]);
  assert.deepEqual(nextSort(s, "app", false), [{ key: "app", dir: 1 }]);
});

test("the repository table keeps every stack, filters by text, stack and never-drilled, and sorts within a stack", () => {
  const now = at("2026-10-03T08:00:00+02:00");
  const repo = (
    /** @type {string} */ owner,
    /** @type {number} */ n,
    drill = false,
  ) => ({
    owner,
    snapshot_count: n,
    snapshots: [],
    ...(drill ? { drill: { last_attempt: 1, last_pass: 1 } } : {}),
  });
  const stacks = {
    gateway: ok([], [repo("traefik", 4, true), repo("crowdsec", 9)]),
    films: ok([]),
    notes: { status: /** @type {const} */ ("pending") },
  };
  const view = {
    stacks: new Set(),
    q: "",
    undrilled: false,
    sort: [],
    collapsed: new Set(),
    open: null,
  };
  const g = repoGroups(stacks, view, now);
  assert.deepEqual(
    g.map((x) => x.stack),
    ["films", "gateway", "notes"],
  );
  const sorted = repoGroups(
    stacks,
    { ...view, sort: [{ key: "snaps", dir: -1 }] },
    now,
  );
  assert.deepEqual(
    sorted[1].rows.map((r) => r.owner),
    ["crowdsec", "traefik"],
  );
  const text = repoGroups(stacks, { ...view, q: "CROWD" }, now);
  assert.deepEqual(
    text.map((x) => [x.stack, x.rows.map((r) => r.owner)]),
    [["gateway", ["crowdsec"]]],
  );
  const never = repoGroups(stacks, { ...view, undrilled: true }, now);
  assert.deepEqual(
    never.map((x) => [x.stack, x.rows.map((r) => r.owner)]),
    [["gateway", ["crowdsec"]]],
  );
  const one = repoGroups(stacks, { ...view, stacks: new Set(["films"]) }, now);
  assert.deepEqual(
    one.map((x) => x.stack),
    ["films"],
  );
});

test("the page's state lives in its address and comes back from it", () => {
  const v = viewFromSearch(
    "?night=2026-09-30&stacks=kp-soft,gateway&drills=never&q=traefik&sort=age:desc,app:asc&section=removed",
  );
  assert.equal(v.night, "2026-09-30");
  assert.deepEqual([...v.stacks], ["kp-soft", "gateway"]);
  assert.equal(v.undrilled, true);
  assert.equal(v.q, "traefik");
  assert.deepEqual(v.sort, [
    { key: "age", dir: -1 },
    { key: "app", dir: 1 },
  ]);
  assert.equal(v.section, "removed");
  assert.deepEqual(searchFromView(v), {
    night: "2026-09-30",
    stacks: "gateway,kp-soft",
    drills: "never",
    q: "traefik",
    sort: "age:desc,app:asc",
  });
  assert.equal(viewFromSearch("?night=yesterday").night, null);
  const empty = searchFromView(viewFromSearch(""));
  assert.deepEqual(empty, {
    night: null,
    stacks: null,
    drills: null,
    q: null,
    sort: null,
  });
});

test("a pinned night further back pages the heatmap so it is on screen", () => {
  assert.equal(offsetFor("2026-10-02", "2026-10-01", 30), 0);
  assert.equal(offsetFor("2026-10-02", "2026-09-03", 30), 0);
  assert.equal(offsetFor("2026-10-02", "2026-09-02", 30), 30);
  assert.equal(offsetFor("2026-10-02", "2026-10-05", 30), 0);
});
