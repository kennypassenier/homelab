// feat-kpadopt-2 (kp-themes 11.0.0, the approved app-homelab-backups demo):
// what the Backups page hands to the package's heat grid, calendar, status
// list and stack bar, worked out without a DOM.
import { test } from "node:test";
import assert from "node:assert/strict";
import { at as moment } from "./support/clock.js";

// Nights are the viewer's own: pin the zone the dashboard shows (the clocks
// are Europe/Brussels; summer time runs until 2026-10-25).
process.env.TZ = "Europe/Brussels";

const {
  calendarDays,
  lateStart,
  monthNights,
  nightCell,
  nightColumns,
  nightRows,
  nightSentence,
  nightSummary,
  nightsSpec,
  repoPeriods,
  stackSizes,
} = await import("../js/backupgrid.js");
const { nightKey } = await import("../js/backupsview.js");

/**
 * The 03:10 round of a night in Brussels summer time: a night carries its
 * evening's date, so its snapshot is made the next morning.
 * @param {string} night
 */
const at = (night, hh = 3, mm = 10) => {
  const [y, m, d] = night.split("-").map(Number);
  const t = new Date(Date.UTC(y, m - 1, d + 1));
  const day = `${t.getUTCFullYear()}-${String(t.getUTCMonth() + 1).padStart(2, "0")}-${String(t.getUTCDate()).padStart(2, "0")}`;
  return moment(
    `${day}T${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}:00+02:00`,
  );
};

const now = { last: "2026-10-18", current: "2026-10-19" };

/** @type {Record<string, any>} */
const reads = {
  kyu: {
    status: "ok",
    native: false,
    noBackup: false,
    reason: null,
    times: [at("2026-10-17"), at("2026-10-18")],
    repos: [
      {
        owner: "kyu",
        snapshot_count: 2,
        snapshots: [
          { id: "aa11", short_id: "aa11", time: at("2026-10-18") },
          { id: "bb22", short_id: "bb22", time: at("2026-10-17") },
        ],
      },
    ],
  },
  inbox: {
    status: "ok",
    native: false,
    noBackup: false,
    reason: "disk quota exceeded",
    times: [at("2026-10-16")],
    repos: [],
  },
  syncthing: {
    status: "ok",
    native: false,
    noBackup: true,
    reason: null,
    times: [],
    repos: [],
  },
};

test("nightCell: backed up names the snapshot time and every app's id; a miss names the host's reason", () => {
  const ok = nightCell("kyu", reads.kyu, "2026-10-18", now);
  assert.equal(ok.tone, "ok");
  assert.match(String(ok.tip), /snapshot at 03:10/);
  assert.match(String(ok.tip), /kyu aa11/);
  const miss = nightCell("inbox", reads.inbox, "2026-10-18", now);
  assert.equal(miss.tone, "bad");
  assert.match(String(miss.tip), /disk quota exceeded/);
  assert.equal(
    nightCell("syncthing", reads.syncthing, "2026-10-18", now).tone,
    "none",
  );
  assert.equal(nightCell("kyu", reads.kyu, "2026-10-10", now).tone, "before");
  assert.equal(
    nightCell("kyu", { status: "pending" }, "2026-10-18", now).tone,
    "loading",
  );
});

test("nightColumns: the quiet weekend keeps its column but carries no day number", () => {
  // 2026-10-17 is a Saturday, 18 a Sunday, 19 a Monday.
  const cols = nightColumns(
    ["2026-10-17", "2026-10-18", "2026-10-19"],
    "quiet",
    1,
  );
  assert.deepEqual(
    cols.map((c) => c.label),
    ["", "", "19"],
  );
  const plain = nightColumns(
    ["2026-10-17", "2026-10-18", "2026-10-19"],
    "plain",
    1,
  );
  assert.deepEqual(
    plain.map((c) => c.label),
    ["17", "18", "19"],
  );
});

test("nightsSpec: a row per stack, a whole-fleet row that fills by the share, the pressed stacks only when asked", () => {
  const spec = nightsSpec({
    names: ["inbox", "kyu", "syncthing"],
    reads,
    nights: ["2026-10-17", "2026-10-18"],
    now,
    weekend: "plain",
    every: 1,
    markOf: () => "red",
    fleetReady: true,
    label: "x",
  });
  assert.equal(spec.rows?.length, 3);
  assert.equal(
    spec.rows?.[2].muted,
    true,
    "a stack that keeps no data is quiet",
  );
  const fleet = spec.summary?.cells[1];
  assert.equal(fleet?.tone, "warn");
  assert.equal(fleet?.share, 0.5);
  assert.equal(fleet?.label, "1 of 2 stacks backed up");
  const only = nightsSpec({
    names: ["inbox", "kyu"],
    reads,
    nights: ["2026-10-18"],
    now,
    weekend: "plain",
    every: 1,
    markOf: () => "red",
    fleetReady: true,
    label: "x",
    shown: new Set(["kyu"]),
  });
  assert.deepEqual(
    only.rows?.map((r) => r.key),
    ["kyu"],
  );
});

test("nightSentence: names the stacks that missed the night and why, exact counts", () => {
  assert.match(
    nightSentence(reads, "2026-10-18", now),
    /^18\/10\/2026 pinned: 1 of 2 stacks backed up\. Missed: inbox \(disk quota exceeded\)\.$/,
  );
  assert.match(
    nightSentence(reads, "2026-10-19", now),
    /has not run yet|pinned/,
  );
});

test("nightRows: a miss offers its repair by name, a late start says so, a stack that keeps no data is not counted", () => {
  const rows = nightRows(
    ["inbox", "kyu", "syncthing"],
    reads,
    "2026-10-18",
    now,
  );
  const byId = Object.fromEntries(rows.map((r) => [r.id, r]));
  assert.equal(byId.inbox.state, "failed");
  assert.equal(byId.inbox.fix, "Back up inbox now");
  assert.match(String(byId.inbox.detail), /disk quota exceeded/);
  assert.equal(byId.kyu.state, "ok");
  assert.equal(byId.syncthing.state, "empty");
  const late = nightRows(
    ["kyu"],
    { kyu: { ...reads.kyu, times: [at("2026-10-18", 3, 40)] } },
    "2026-10-18",
    now,
  );
  assert.equal(late[0].state, "warn");
  assert.equal(late[0].fix, "Started late");
});

test("nightSummary: 'N of M backed up' without the stacks that keep no data, 'N of M stacks read' while reading", () => {
  assert.equal(
    nightSummary(
      { ok: 11, warn: 1, failed: 1, empty: 1, loading: 0, total: 14 },
      "2026-10-18",
    ),
    "12 of 13 backed up on 18/10/2026",
  );
  assert.equal(
    nightSummary(
      { ok: 0, warn: 0, failed: 0, empty: 0, loading: 2, total: 13 },
      "2026-10-18",
    ),
    "11 of 13 stacks read",
  );
});

test("lateStart: a round that begins after 03:35 is late", () => {
  assert.equal(lateStart(at("2026-10-18", 3, 34)), false);
  assert.equal(lateStart(at("2026-10-18", 3, 40)), true);
  assert.equal(lateStart(moment("2026-10-18T14:00:00+02:00")), false);
});

test("calendarDays: every day of the range, loading while a stack is pending, not run yet after the last night", () => {
  const range = { from: "2026-10-16", to: "2026-10-20" };
  const days = calendarDays(reads, now, range);
  assert.deepEqual(Object.keys(days), [
    "2026-10-16",
    "2026-10-17",
    "2026-10-18",
    "2026-10-19",
    "2026-10-20",
  ]);
  assert.equal(days["2026-10-18"].tone, "warn");
  assert.equal(days["2026-10-18"].count, "−1");
  assert.equal(days["2026-10-20"].tone, "future");
  const reading = calendarDays(
    { ...reads, kyu: { status: "pending" } },
    now,
    range,
  );
  assert.equal(reading["2026-10-18"].tone, "loading");
});

test("monthNights: one entry per day of the month", () => {
  assert.equal(monthNights(2026, 10).length, 31);
  assert.equal(monthNights(2026, 2).length, 28);
  assert.equal(monthNights(2028, 2).length, 29);
  assert.equal(monthNights(2026, 10)[0], "2026-10-01");
});

test("repoPeriods: seven nights ending last night; a night with a snapshot is up, a missed one down", () => {
  const periods = repoPeriods(reads.kyu, reads.kyu.repos[0], now);
  assert.equal(periods.length, 7);
  assert.equal(periods[6].state, "up");
  assert.equal(periods[5].state, "up");
  assert.equal(periods[0].state, "none", "before this stack had any history");
  assert.equal(nightKey(at("2026-10-18")), "2026-10-18");
});

test("stackSizes: the stacks biggest first, a repository without a measured size counts as none", () => {
  const sizes = stackSizes([
    { stack: "a", size_bytes: 10 },
    { stack: "b", size_bytes: 500 },
    { stack: "a", size_bytes: null },
  ]);
  assert.deepEqual(
    sizes.map((s) => [s.stack, s.bytes, s.count]),
    [
      ["b", 500, 1],
      ["a", 10, 2],
    ],
  );
});
