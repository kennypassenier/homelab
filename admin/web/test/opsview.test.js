// feat-kpadopt-2 (kp-themes 11.0.0, the approved app-homelab-backups demo):
// the pure parts of the System landing and the host log's picture of its
// window, driven without a browser.
import { test } from "node:test";
import assert from "node:assert/strict";
import { at } from "./support/clock.js";

// Nights are the viewer's own: pin the zone the dashboard shows.
process.env.TZ = "Europe/Brussels";

const { backupsLastNight } = await import("../js/systemview.js");
const { BUCKET_S, bucketStart, densityBuckets, inRange } =
  await import("../js/hostlogview.js");

test("backupsLastNight: counts the stacks that kept data and names the ones that missed the night", () => {
  // 03:10 on 18/10 belongs to the night of 17/10; now is 19/10 12:00, so
  // last night is the 18th (its round runs at 03:10 on the 19th).
  const lastRound = at("2026-10-19T03:10:00+02:00");
  const dayBefore = at("2026-10-18T03:10:00+02:00");
  const x = backupsLastNight(
    {
      stacks: { a: [dayBefore, lastRound], b: [dayBefore], c: [] },
      no_backup: ["c"],
    },
    ["a", "b", "c"],
    at("2026-10-19T12:00:00+02:00"),
  );
  assert.deepEqual(x, { covered: 1, expected: 2, missed: ["b"] });
});

test("densityBuckets: every five minutes from the first line to the last, the worst level in each", () => {
  const t = at("2026-10-19T03:10:00+02:00");
  const lines = [
    {
      seq: 1,
      ts: t,
      level: "info",
      source: "HOST",
      msg: "a",
      req: null,
      by: null,
    },
    {
      seq: 2,
      ts: t + 10,
      level: "error",
      source: "admin",
      msg: "b",
      req: null,
      by: null,
    },
    {
      seq: 3,
      ts: t + 2 * BUCKET_S + 5,
      level: "warn",
      source: "admin",
      msg: "c",
      req: null,
      by: null,
    },
  ];
  const b = densityBuckets(lines);
  assert.equal(b.length, 3, "the quiet five minutes in between stay");
  assert.equal(b[0].worst, "critical");
  assert.equal(b[0].first, 1);
  assert.equal(b[0].bySource.get("HOST")?.worst, "info");
  assert.equal(b[1].worst, "none");
  assert.equal(b[1].count, 0);
  assert.equal(b[2].worst, "warning");
  assert.deepEqual(densityBuckets([]), []);
});

test("inRange: a line falls in the five minutes that start at a bucket", () => {
  const t = at("2026-10-19T03:10:00+02:00");
  const start = bucketStart(t);
  assert.equal(start % BUCKET_S, 0);
  const line = {
    seq: 1,
    ts: start + 299,
    level: "info",
    source: "x",
    msg: "",
    req: null,
    by: null,
  };
  assert.equal(inRange(line, start), true);
  assert.equal(inRange({ ...line, ts: start + 300 }, start), false);
  assert.equal(inRange(line, null), true);
});
