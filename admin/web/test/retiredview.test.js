// feat-retired-1: the Retired page's pure view-model, driven without a DOM
// or a network — same shape the host's `GetRetired` answers with.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  fullyRemovable,
  operationLabel,
  repoTotals,
  splitInUse,
} from "../js/retiredview.js";

test("operationLabel names the three ways something leaves the files", () => {
  assert.equal(operationLabel("stack"), "stack destroyed or forgotten");
  assert.equal(operationLabel("app"), "app dropped from its stack");
  assert.equal(operationLabel("unit"), "native unit dropped from its stack");
  // An unknown kind is shown as-is rather than hidden or thrown on — a
  // forward-compat seam, not a crash, if the host ever adds a fourth kind.
  assert.equal(operationLabel("mystery"), "mystery");
});

test("repoTotals sums snapshots and size, and tracks the newest across repos", () => {
  const totals = repoTotals([
    {
      name: "jellyfin-config",
      in_use: false,
      snapshot_count: 5,
      size_bytes: 1000,
      newest_snapshot: { short_id: "abc123", time: 100 },
      measured_at: 200,
    },
    {
      name: "sonarr-config",
      in_use: false,
      snapshot_count: 3,
      size_bytes: 500,
      newest_snapshot: { short_id: "def456", time: 300 },
      measured_at: 400,
    },
  ]);
  assert.deepEqual(totals, {
    repoCount: 2,
    snapshotCount: 8,
    sizeBytes: 1500,
    newestTime: 300,
    unread: 0,
  });
});

test("repoTotals: a repository the cache has never read counts as unread, not zero", () => {
  const totals = repoTotals([
    {
      name: "never-read-config",
      in_use: false,
      snapshot_count: 0,
      size_bytes: null,
      newest_snapshot: null,
      measured_at: null,
    },
  ]);
  assert.equal(totals.unread, 1);
  assert.equal(totals.newestTime, null);
  assert.equal(totals.sizeBytes, 0);
});

test("repoTotals of no repositories at all is all zero, not an error", () => {
  assert.deepEqual(repoTotals([]), {
    repoCount: 0,
    snapshotCount: 0,
    sizeBytes: 0,
    newestTime: null,
    unread: 0,
  });
});

test("splitInUse keeps what a managed stack still uses and removes the rest", () => {
  const paths = ["/appdata/drill/drill-config", "/appdata/other/shared-config"];
  const inUse = ["/appdata/other/shared-config"];
  assert.deepEqual(splitInUse(paths, inUse), {
    kept: ["/appdata/other/shared-config"],
    removed: ["/appdata/drill/drill-config"],
  });
});

test("splitInUse with nothing in use removes everything", () => {
  const paths = ["/appdata/drill/drill-config"];
  assert.deepEqual(splitInUse(paths, []), {
    kept: [],
    removed: ["/appdata/drill/drill-config"],
  });
});

test("fullyRemovable is true only when in_use is empty", () => {
  assert.equal(fullyRemovable({ in_use: [] }), true);
  assert.equal(fullyRemovable({ in_use: ["shared-config"] }), false);
});
