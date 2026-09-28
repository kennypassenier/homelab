// feat-overview-4 and the state column, driven without a browser.
import { test } from "node:test";
import assert from "node:assert/strict";
import { measuredAgo, ramText, stackState } from "../js/fleet.js";

const base = { name: "media", vmid: 106, apps_running: 2, apps_total: 2 };

test("a parked stack reads parked even when it is offline", () => {
  assert.deepEqual(stackState({ ...base, online: false, enabled: false }), {
    label: "parked",
    tone: "warn",
  });
});

test("an online stack with an app down reads degraded", () => {
  assert.equal(
    stackState({ ...base, online: true, enabled: true, apps_running: 1 }).label,
    "degraded",
  );
});

test("measured ago uses seconds, then minutes, then hours", () => {
  assert.equal(measuredAgo(100, 112), "measured 12 s ago");
  assert.equal(measuredAgo(100, 100 + 125), "measured 2 min 5 s ago");
  assert.equal(measuredAgo(0, 3 * 3600 + 120), "measured 3 h 2 min ago");
  assert.equal(measuredAgo(200, 100), "measured 0 s ago");
});

test("ram reads in GB, and a dash before the first reading", () => {
  assert.equal(
    ramText({
      ...base,
      online: true,
      enabled: true,
      ram_used_mb: 1946,
      ram_max_mb: 5120,
    }),
    "1.9 / 5.0 GB",
  );
  assert.equal(
    ramText({
      ...base,
      online: true,
      enabled: true,
      ram_used_mb: null,
      ram_max_mb: null,
    }),
    "—",
  );
});
