// redesign-final (3.71.0's final review round): cases that failed on the
// integration branch redesign-371 itself (c6334fc9), fixed here.
import { test } from "node:test";
import assert from "node:assert/strict";
import { hostChecks } from "../js/host.js";

// The Doctor's report lives in Host's checks card since 3.71.0; a run's
// first answer from the host carries no checks yet (`{report: {},
// refreshing}`), which threw and stopped the read for good.
test("redesign-final-base: Host's checks read a report without checks as none, so the read goes on", () => {
  assert.deepEqual(hostChecks(/** @type {any} */ ({})), []);
  assert.deepEqual(
    hostChecks(
      /** @type {any} */ ({
        checks: [
          { name: "disk", health: "ok", detail: "" },
          { name: "stack gateway", health: "ok", detail: "" },
        ],
      }),
    ).map((c) => c.name),
    ["disk"],
  );
});
