// fix-179: the per-stack progress/fold helpers shared by the backup
// calendar (fix-177) and the Backups page, driven without a DOM or a
// network. backupcalendar.test.js already exercises these through the
// calendar's own re-exported names; this file drives the shared module
// directly, and with a second page's shape (no `times`, no `empty`), to
// prove it carries no calendar-specific assumption.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  stackChips,
  stackReadProgress,
  withStackResult,
} from "../js/perstack.js";

test("withStackResult folds one outcome in without touching the rest", () => {
  const a = { media: { status: "pending" } };
  const b = withStackResult(a, "backups", { status: "pending" });
  assert.deepEqual(
    a,
    { media: { status: "pending" } },
    "the input is untouched",
  );
  assert.deepEqual(b, {
    media: { status: "pending" },
    backups: { status: "pending" },
  });
});

test("stackReadProgress: nothing settled yet is 0%, not done", () => {
  const p = stackReadProgress({
    media: { status: "pending" },
    backups: { status: "pending" },
  });
  assert.deepEqual(p, { total: 2, loaded: 0, pct: 0, done: false, failed: [] });
});

test("stackReadProgress: a page's own successful status counts as loaded, failed is named", () => {
  // The Backups page's own shape (fix-179): "ok" carries rows, not "times"
  // — the shared helper never looks inside a non-"pending"/"failed" status.
  const p = stackReadProgress({
    media: { status: "ok", native: false, repos: [] },
    backups: {
      status: "failed",
      reason: "no answer from the host within 170 s",
    },
    jellyfin: { status: "pending" },
  });
  assert.equal(p.total, 3);
  assert.equal(p.loaded, 2);
  assert.equal(p.pct, 67);
  assert.equal(p.done, false);
  assert.deepEqual(p.failed, [
    { stack: "backups", reason: "no answer from the host within 170 s" },
  ]);
});

test("stackReadProgress: every stack settled is done, and failures sort by name", () => {
  const p = stackReadProgress({
    zulu: { status: "failed", reason: "timeout" },
    alpha: { status: "failed", reason: "host down" },
  });
  assert.equal(p.done, true);
  assert.deepEqual(
    p.failed.map((f) => f.stack),
    ["alpha", "zulu"],
  );
});

test("stackReadProgress of no stacks at all is 0%, not done (nothing to wait for)", () => {
  assert.deepEqual(stackReadProgress({}), {
    total: 0,
    loaded: 0,
    pct: 0,
    done: false,
    failed: [],
  });
});

// ── fix-224: named per-stack chips, not a bare fraction ─────────────────

test('stackChips: a settled stack reads "read", sorted by name', () => {
  const rows = stackChips(
    {
      zulu: { status: "ok", times: [] },
      alpha: { status: "ok", times: [] },
    },
    {},
    1_000,
  );
  assert.deepEqual(rows, [
    { stack: "alpha", state: "read" },
    { stack: "zulu", state: "read" },
  ]);
});

test('stackChips: a pending stack reads "reading" with how long it has waited', () => {
  const rows = stackChips(
    { inbox: { status: "pending" } },
    { inbox: 1_000 },
    1_000 + 37_000,
  );
  assert.deepEqual(rows, [{ stack: "inbox", state: "reading", seconds: 37 }]);
});

test("stackChips: a pending stack with no recorded start reads 0 s, not a crash", () => {
  const rows = stackChips({ inbox: { status: "pending" } }, {}, 5_000);
  assert.deepEqual(rows, [{ stack: "inbox", state: "reading", seconds: 0 }]);
});

test('stackChips: no_backup is its own terminal state, never "reading" or "failed"', () => {
  const rows = stackChips({ registry: { status: "no_backup" } }, {}, 0);
  assert.deepEqual(rows, [{ stack: "registry", state: "no_backup" }]);
});

test("stackChips: a failed (including timed-out) stack carries the host's own reason", () => {
  const rows = stackChips(
    {
      inbox: {
        status: "failed",
        reason: "did not answer within 180 s",
      },
    },
    {},
    0,
  );
  assert.deepEqual(rows, [
    { stack: "inbox", state: "failed", reason: "did not answer within 180 s" },
  ]);
});
