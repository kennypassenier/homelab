// kp11 Activity (the approved demo's section 4): the Jobs timeline's lanes,
// bars, waiting bands and the three figures under it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { jobFigures, jobTimeline } from "../js/jobs.js";

const NOW = 1_790_000_000;

/** @returns {import("../js/jobs.js").Job} */
const job = (/** @type {Partial<import("../js/jobs.js").Job>} */ o) => ({
  job: 1,
  origin: { from: "manual" },
  stack: "immich",
  action: "update",
  args: {},
  state: "done",
  queued_at: NOW - 3000,
  started_at: NOW - 2900,
  finished_at: NOW - 2000,
  reqs: [],
  message: null,
  cli: null,
  progress: null,
  restarts_dashboard: false,
  ...o,
});

const label = (/** @type {string} */ a) => a;

test("kp11-activity: the Jobs timeline has a lane per stack and the wait is a band, not a bar", () => {
  const jobs = [
    job({ job: 1 }),
    job({
      job: 2,
      stack: "_host",
      queued_at: NOW - 600,
      started_at: NOW - 595,
    }),
    job({
      job: 3,
      state: "queued",
      queued_at: NOW - 50,
      started_at: null,
      finished_at: null,
    }),
    job({ job: 4, state: "failed", message: "boom" }),
    job({
      job: 5,
      stack: "old",
      queued_at: NOW - 9e5,
      started_at: NOW - 9e5,
      finished_at: NOW - 9e5 + 10,
    }),
  ];
  const m = jobTimeline(jobs, label, NOW, NOW - 6 * 3600);
  assert.deepEqual(
    m.lanes.map((l) => l.id),
    ["host", "immich"],
    "a stack with nothing in the last 24 hours has no lane",
  );
  assert.equal(m.runs.length, 4);
  const byId = Object.fromEntries(m.runs.map((r) => [r.id, r]));
  assert.equal(byId["job-3"].state, "queued");
  assert.equal(
    byId["job-3"].start,
    (NOW - 50) * 1000,
    "a ghost starts when it was queued",
  );
  assert.equal(byId["job-3"].end, undefined);
  assert.equal(byId["job-4"].state, "failed");
  assert.equal(
    byId["job-1"].start,
    (NOW - 2900) * 1000,
    "a bar starts when the job did",
  );
  // Job 1 waited 100 s (a band), job 2 only 5 s (none).
  assert.deepEqual(m.zones.map((z) => z.id).sort(), ["wait-1", "wait-4"]);
  assert.match(m.zones[0].label, /^waited 1 min 40 s$/);
});

test("kp11-activity: the Jobs timeline's figures count the view, the live jobs and the longest wait", () => {
  const jobs = [
    job({ job: 1 }),
    job({
      job: 3,
      state: "queued",
      queued_at: NOW - 50,
      started_at: null,
      finished_at: null,
    }),
  ];
  const f = jobFigures(jobs, NOW, { from: NOW - 6 * 3600, to: NOW });
  assert.equal(f.inView, 2);
  assert.equal(f.live, 1);
  assert.equal(f.longestWaitS, 100);
  assert.equal(jobFigures(jobs, NOW, { from: NOW - 10, to: NOW }).inView, 1);
});
