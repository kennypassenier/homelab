// one-process-batch-1 (Kenny, 2026-10-23): a batch is ONE process. Its step
// plan is the concatenation of every member's plan, its total the sum, known
// before the first step; its progress counts across the members.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  batchOf,
  batchProcess,
  batchState,
  memberLength,
} from "../js/batchprocess.js";
import { jobSteps, stepFigures } from "../js/jobs.js";

/** @param {number} job @param {string} state @param {any} [p] @param {any} [extra] */
const job = (job, state, p = null, extra = {}) =>
  /** @type {any} */ ({
    job,
    origin: { from: "batch", batch: 9 },
    stack: `s${job}`,
    action: "deploy",
    args: {},
    state,
    queued_at: 100,
    started_at: state === "queued" ? null : 101,
    finished_at: ["done", "failed", "refused", "deferred"].includes(state)
      ? 150
      : null,
    reqs: [],
    message: null,
    cli: null,
    progress: p,
    restarts_dashboard: false,
    ...extra,
  });
/** @param {number} n @param {number} m @param {string} [step] @param {boolean} [finished] */
const prog = (n, m, step = "x", finished = false) => ({
  op: "deploy",
  step,
  n,
  m,
  finished,
  changed: false,
  expected_step_s: null,
  expected_total_s: null,
  expected_remaining_s: null,
  elapsed_s: 5,
  runs: 0,
});
/** @param {boolean} [exact] @returns {any} */
const batch = (exact = true) => ({
  batch: 9,
  jobs: [
    { job: 1, stack: "kitchen", state: "queued", message: null },
    { job: 2, stack: "gateway", state: "queued", message: null },
  ],
  done: 0,
  ok: 0,
  failed: 0,
  deferred: 0,
  plan: [
    {
      job: 1,
      stack: "kitchen",
      steps: ["gates", "provision container"],
      exact,
    },
    {
      job: 2,
      stack: "gateway",
      steps: ["gates", "deploy", "verify"],
      exact: true,
    },
  ],
  steps_total: 5,
  steps_exact: exact,
});

test("before the first step the total is the sum of every plan and the rows are named per stack", () => {
  const r = batchProcess(batch(), [job(1, "queued"), job(2, "queued")], 200);
  assert.equal(r.total, 5);
  assert.equal(r.job.progress?.m, 5);
  assert.equal(r.job.progress?.n, 0);
  assert.deepEqual(
    r.rows.map((x) => x.title),
    [
      "kitchen · gates",
      "kitchen · provision container",
      "gateway · gates",
      "gateway · deploy",
      "gateway · verify",
    ],
  );
  assert.ok(r.rows.every((x) => x.state === "wait"));
  assert.equal(r.job.state, "queued");
  assert.equal(r.approx, false);
});

test("progress counts across members: the second member's step 1 is the batch's step 3", () => {
  const jobs = [
    job(1, "done", prog(2, 2, "provision container", true)),
    job(2, "running", prog(1, 3, "gates")),
  ];
  const r = batchProcess(batch(), jobs, 200);
  assert.equal(r.job.progress?.n, 3);
  assert.equal(r.job.progress?.m, 5);
  assert.equal(r.job.progress?.step, "gateway · gates");
  assert.equal(r.current, 2);
  assert.equal(r.job.state, "running");
  assert.deepEqual(
    r.rows.map((x) => x.state),
    ["ok", "ok", "run", "wait", "wait"],
  );
  assert.equal(stepFigures(r.job).done, 2);
});

test("between two members nothing falls back to 0 of: the finished member counts and the total stays", () => {
  const jobs = [job(1, "done", prog(2, 2, "x", true)), job(2, "queued")];
  const r = batchProcess(batch(), jobs, 200);
  assert.equal(r.job.state, "running");
  assert.equal(r.job.progress?.m, 5);
  assert.equal(r.job.progress?.n, 2);
  assert.equal(stepFigures(r.job).done, 2);
});

test("a member's announced count replaces its planned one, the total follows", () => {
  const jobs = [job(1, "running", prog(1, 4, "gates")), job(2, "queued")];
  assert.equal(memberLength(batch().plan[0], jobs[0]), 4);
  assert.equal(batchProcess(batch(), jobs, 200).total, 7);
});

test("a failed member stops there, its remaining steps are not run, and the batch fails when all ended", () => {
  const jobs = [
    job(1, "failed", prog(2, 2, "provision container"), { message: "boom" }),
    job(2, "done", prog(3, 3, "verify", true)),
  ];
  const r = batchProcess(batch(), jobs, 200);
  assert.equal(r.job.state, "failed");
  assert.deepEqual(
    r.rows.map((x) => x.state),
    ["ok", "bad", "ok", "ok", "ok"],
  );
  assert.match(r.job.message ?? "", /kitchen: boom/);
});

test("a member that never ran (refused) counts as skipped steps, not as a gap", () => {
  const jobs = [job(1, "refused"), job(2, "done", prog(3, 3, "verify", true))];
  const r = batchProcess(batch(), jobs, 200);
  assert.deepEqual(
    r.rows.map((x) => x.state),
    ["skip", "skip", "ok", "ok", "ok"],
  );
  assert.equal(r.job.progress?.n, r.job.progress?.m);
});

test("an estimated member makes the total 'about N', never a blank", () => {
  const r = batchProcess(
    batch(false),
    [job(1, "queued"), job(2, "queued")],
    200,
  );
  assert.equal(r.approx, true);
  assert.equal(batchOf(r.total, r.approx), "about 5");
  assert.equal(batchOf(5, false), "5");
  assert.equal(batchOf(null, true), "not counted yet");
});

test("a step name the member sent wins over an ESTIMATED planned one", () => {
  const seen = new Map();
  const jobs = [job(1, "running", prog(1, 2, "gate check")), job(2, "queued")];
  const r = batchProcess(batch(false), jobs, 200, seen);
  assert.equal(r.rows[0].title, "kitchen · gate check");
});

test("an exact plan names every step before it is reached", () => {
  const r = batchProcess(batch(), [job(1, "queued"), job(2, "queued")], 200);
  assert.deepEqual(
    r.rows.map((x) => x.title),
    [
      "kitchen · gates",
      "kitchen · provision container",
      "gateway · gates",
      "gateway · deploy",
      "gateway · verify",
    ],
  );
  assert.deepEqual(r.because, []);
});

test("an estimated member says why in 'because'", () => {
  const b = batch(false);
  b.plan[0].estimated_because = [
    "patch: the host's managed stacks were not read",
  ];
  const r = batchProcess(b, [job(1, "queued"), job(2, "queued")], 200);
  assert.deepEqual(r.because, [
    "kitchen: patch: the host's managed stacks were not read",
  ]);
});

test("a job's own plan names its steps from the first frame", () => {
  const j = job(7, "running", prog(0, 3, ""), {
    plan: ["kitchen · gates", "kitchen · deploy", "gateway · gates"],
  });
  assert.deepEqual(
    jobSteps(j, new Map()).map((r) => r.title),
    ["kitchen · gates", "kitchen · deploy", "gateway · gates"],
  );
  // A plan of another length than the host's total is not trusted.
  const off = job(7, "running", prog(0, 4, ""), { plan: ["a", "b", "c"] });
  assert.deepEqual(
    jobSteps(off, new Map()).map((r) => r.title),
    ["Step 1", "Step 2", "Step 3", "Step 4"],
  );
});

test("batchState folds the members' states into one", () => {
  assert.equal(batchState(["queued", "queued"]), "queued");
  assert.equal(batchState(["done", "queued"]), "running");
  assert.equal(batchState(["done", "done"]), "done");
  assert.equal(batchState(["done", "failed"]), "failed");
  assert.equal(batchState(["deferred", "deferred"]), "deferred");
});
