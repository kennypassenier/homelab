// redesign-final (B, coordinator 2026-10-04): no test, harness or demo
// host reads the real clock, and no test names a date that expires; the
// gate runs this (the node suite), the commit hook runs the script itself.
import { test } from "node:test";
import assert from "node:assert/strict";
import { scan } from "../scripts/check-test-clock.mjs";

test("redesign-final: no test, harness or demo host reads the real clock or names a date after today", () => {
  assert.deepEqual(scan(), []);
  // It sees what it must: a future date counts after the day it names.
  assert.ok(
    scan({ today: "2026-01-01" }).some((f) => /lies after today/.test(f)),
    "a date after an early today is not found",
  );
});
