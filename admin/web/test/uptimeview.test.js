import assert from "node:assert/strict";
import { test } from "node:test";
import {
  periodOf,
  shareText,
  tileEntry,
  tileKey,
  uptimeQuery,
  zoneOffsetS,
} from "../js/uptimeview.js";

test("Brussels is one or two hours ahead of UTC", () => {
  assert.equal(zoneOffsetS(Date.parse("2026-01-15T12:00:00Z") / 1000), 3600);
  assert.equal(zoneOffsetS(Date.parse("2026-07-15T12:00:00Z") / 1000), 7200);
  assert.match(
    uptimeQuery("day", 1_784_000_000),
    /hours=2160&bucket_s=86400&offset_s=\d+/,
  );
});

test("a bucket reads as a strip cell with its figures", () => {
  const p = periodOf(
    {
      start: 1_784_000_000,
      state: "degraded",
      uptime: 0.995,
      up: 199,
      dep: 0,
      down: 1,
    },
    "day",
  );
  assert.equal(p.state, "degraded");
  assert.match(p.tip ?? "", /99\.5 % of 200 readings/);
  assert.equal(
    periodOf({ start: 0, state: "none", uptime: null }, "hour").state,
    "none",
  );
});

test("a tile reads its own key, else its stack's", () => {
  const keys = new Map([["stack:media", 1]]);
  assert.equal(tileKey(keys, { host: "a.x", stack: "media" }), "stack:media");
  keys.set("tile:a.x", 1);
  assert.equal(tileKey(keys, { host: "a.x", stack: "media" }), "tile:a.x");
  assert.equal(shareText(null, "90 days"), "no readings yet");
});

test("a young own key borrows its stack's older days, never the other way round", () => {
  /**
   * @param {number} start @param {number} up @param {number} [down]
   * @returns {import("../js/uptimeview.js").Bucket}
   */
  const day = (start, up, down = 0) => ({
    start,
    state: up + down === 0 ? "none" : down ? "degraded" : "up",
    uptime: up + down ? up / (up + down) : null,
    up,
    dep: 0,
    down,
  });
  const keys = new Map([
    ["tile:a.x", { periods: [day(0, 0), day(86400, 0), day(172800, 10)] }],
    [
      "stack:media",
      { periods: [day(0, 8, 2), day(86400, 10), day(172800, 3, 7)] },
    ],
  ]);
  const e = tileEntry(keys, { host: "a.x", stack: "media" });
  assert.deepEqual(
    e?.periods.map((p) => [p.start, p.up, p.down]),
    [
      [0, 8, 2],
      [86400, 10, 0],
      [172800, 10, 0],
    ],
  );
  assert.equal(e?.uptime, 28 / 30);
  assert.equal(
    tileEntry(new Map([["tile:a.x", { periods: [day(0, 4)] }]]), {
      host: "a.x",
      stack: "none",
    })?.periods.length,
    1,
  );
  assert.equal(tileEntry(new Map(), { host: "a.x", stack: "m" }), undefined);
});
