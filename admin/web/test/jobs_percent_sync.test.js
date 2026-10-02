// Invariant (Kenny, 2026-10-02 11:02, docs/INVARIANTS.md "the progress bar
// and the step counter agree"): "Die twee moeten samen lopen" — the
// progress bar's percentage must equal the step counter's N/M whenever a
// step total exists (e.g. 263/310 reads 85%, never 99%). Time estimates
// (`expected_remaining_s`) only ever feed "Expected remaining"; they must
// never pull the bar's own percentage above what the step fraction says.
//
// `percent()` (js/jobs.js) is being changed by another helper to prefer the
// step fraction over the time-based estimate whenever `p.m` is set; this
// test is written against the invariant, not the helper's patch, and is
// marked `todo` until that patch lands (it fails against today's
// `Math.max(byTime, bySteps)` on purpose: byTime is 99 here while the step
// fraction is 85).
import { test } from "node:test";
import assert from "node:assert/strict";
import { percent } from "../js/jobs.js";

/** @returns {import("../js/jobs.js").Job} */
const job = (/** @type {Partial<import("../js/jobs.js").Job>} */ o = {}) => ({
  job: 1,
  origin: { from: "manual" },
  stack: "media",
  action: "update",
  args: {},
  state: "running",
  queued_at: 900,
  started_at: 1010,
  finished_at: null,
  reqs: [1],
  message: null,
  cli: null,
  progress: null,
  restarts_dashboard: false,
  ...o,
});

/** @returns {import("../js/jobs.js").Progress} */
const prog = (
  /** @type {Partial<import("../js/jobs.js").Progress>} */ o = {},
) => ({
  op: "update-media",
  step: "pull",
  n: 263,
  m: 310,
  finished: false,
  changed: false,
  expected_step_s: 1,
  expected_total_s: 1000,
  expected_remaining_s: 10,
  elapsed_s: 990,
  runs: 5,
  ...o,
});

test(
  "invariants: the percentage equals N/M when a step total exists, never a higher time-based guess",
  {
    todo: "waits for jobs.js percent() to prefer the step fraction over byTime when p.m is set (Kenny, 2026-10-02 11:02)",
  },
  () => {
    // 990 s elapsed, 10 s expected remaining: byTime alone would read 99%.
    // 263/310 steps done (finished: false, so (263-1)/310): the step
    // fraction reads 85%. The two must agree; 85 is the fraction, so the
    // bar must show 85, never 99.
    const j = job({ started_at: 1010, progress: prog() });
    const now = 2000; // 2000 - 1010 = 990 s elapsed
    const progressAt = 2000; // no countdown drift since the last event
    assert.equal(
      percent(j, progressAt, now),
      85,
      "the bar must read the same percentage as the step counter (263/310 = 85%), not a time-based 99%",
    );
  },
);

test("invariants: with no step total, the time-based estimate is still allowed to drive the bar", () => {
  const j = job({ started_at: 1010, progress: prog({ m: null }) });
  // Unchanged behaviour: no step fraction exists, so the time-based
  // estimate is the only input and nothing about this invariant
  // constrains it.
  assert.equal(percent(j, 2000, 2000), 99);
});
