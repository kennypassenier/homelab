// redesign-final (3.71.0's final whole-dashboard review, 2026-10-04): the
// logic behind the review's critical and high findings, one block per
// finding (the layout half lives in the whole-screen cases).
import { test } from "node:test";
import assert from "node:assert/strict";
import * as jobs from "../js/jobs.js";

/** @returns {import("../js/jobs.js").Job} */
const job = (/** @type {Partial<import("../js/jobs.js").Job>} */ o = {}) => ({
  job: 7,
  origin: { from: "manual" },
  stack: "kp-soft",
  action: "deploy",
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
const prog = (/** @type {Partial<import("../js/jobs.js").Progress>} */ o) => ({
  op: "deploy",
  step: "pull",
  n: 1,
  m: 3,
  finished: false,
  changed: false,
  expected_step_s: null,
  expected_total_s: null,
  expected_remaining_s: null,
  elapsed_s: 0,
  runs: 0,
  ...o,
});

// C2: the running-job drawer lists the job's steps (FLOWS.md §1.2), done
// ones ticked, the current one running, the rest waiting, by the names
// the host sent as each step began.
test("redesign-final-c2: a job's step list ticks the done steps, runs the current one and names each by what the host sent", () => {
  const names = new Map([
    [1, "back up"],
    [2, "pull images"],
  ]);
  const rows = jobs.jobSteps(
    job({ progress: prog({ n: 2, m: 3, step: "pull images" }) }),
    names,
  );
  assert.deepEqual(
    rows.map((r) => [r.title, r.state]),
    [
      ["back up", "ok"],
      ["pull images", "run"],
      ["Step 3", "wait"],
    ],
  );
  for (const r of rows) assert.ok(r.desc, `${r.title} has no description`);
});

test("redesign-final-c2: a long job folds the steps far from the current one into two summary rows", () => {
  const rows = jobs.jobSteps(
    job({ progress: prog({ n: 20, m: 35, step: "restart" }) }),
    new Map([[20, "restart"]]),
  );
  assert.ok(rows.length <= 7, `${rows.length} rows`);
  assert.deepEqual(rows[0], {
    title: "Steps 1–18",
    desc: "done",
    state: "ok",
  });
  assert.equal(rows.find((r) => r.state === "run")?.title, "restart");
  assert.equal(rows[rows.length - 1].title, "Steps 23–35");
});

test("redesign-final-c2: a finished job ticks every step, a failed one marks its last step failed, a queued one says it waits", () => {
  const done = jobs.jobSteps(
    job({ state: "done", progress: prog({ n: 3, m: 3, finished: true }) }),
    new Map(),
  );
  assert.ok(done.every((r) => r.state === "ok"));
  const failed = jobs.jobSteps(
    job({ state: "failed", progress: prog({ n: 2, m: 3 }) }),
    new Map(),
  );
  assert.deepEqual(
    failed.map((r) => r.state),
    ["ok", "bad", "skip"],
  );
  const queued = jobs.jobSteps(job({ state: "queued" }), new Map());
  assert.deepEqual(
    queued.map((r) => [r.title, r.state]),
    [["Waiting in the queue", "wait"]],
  );
});

// H2: an action dialog never shows the host's raw error (paths, "os error
// 2", "No preview: exec _host") and never stays armed when its preview
// says it cannot run: one plain sentence and a disabled primary with that
// sentence as its reason.
test("redesign-final-h2: a deploy whose stack file is missing reads as one plain sentence and is not ready", async () => {
  const { previewGate } = await import("../js/previewgate.js");
  const g = previewGate(
    { action: "deploy", submit: "Deploy", stack: "films" },
    {
      cli: null,
      cli_unavailable:
        "cannot read /home/kenny/.cache/pw-tmp/tmp.RN25dJowxS/admin-data/repo/stacks/films/lxc-compose.yml: No such file or directory (os error 2)",
      plan_unavailable:
        "/home/kenny/.cache/pw-tmp/tmp.RN25dJowxS/admin-data/repo/stacks/films has no lxc-compose.yml",
    },
    null,
  );
  assert.equal(g.ready, false);
  assert.match(
    g.reason,
    /^films has no stack file \(lxc-compose\.yml\) in the repository/,
  );
  for (const t of [g.reason, ...g.notes]) {
    assert.doesNotMatch(t, /\/home\/|os error|No CLI line|No plan/, t);
  }
});

test("redesign-final-h2: a refused preview names the action by its label and says what to do, without the route's internal name", async () => {
  const { previewGate } = await import("../js/previewgate.js");
  const g = previewGate(
    { action: "exec", submit: "Run a command", stack: "_host" },
    null,
    {
      what: "exec _host",
      why: "exec needs the container's number",
      fix: "type the vmid, e.g. 105",
    },
  );
  assert.equal(g.ready, false);
  assert.equal(
    g.reason,
    "Run a command needs the container's number: type the vmid, e.g. 105.",
  );
});

test("redesign-final-h2: a clean preview is ready and adds no notes", async () => {
  const { previewGate } = await import("../js/previewgate.js");
  const g = previewGate(
    { action: "deploy", submit: "Deploy", stack: "kp-soft" },
    { cli: "homelab deploy kp-soft", plan: { files: [] } },
    null,
  );
  assert.deepEqual(g, { ready: true, reason: "", notes: [] });
});

test("redesign-final-h2: a host action that targets one container is titled for a container, and inline code in a description is code", async () => {
  const { actionForm } = await import("../js/actionforms.js");
  const { codeSpans } = await import("../js/previewgate.js");
  const f = actionForm(
    {
      action: "exec",
      label: "Run a command",
      target: "host",
      args: ["vmid", "command"],
      what: "x",
      scope: "operate",
      needs: "",
      confirm: false,
      refused_for_self: false,
      destructive: false,
    },
    { stack: "", selfStack: "admin" },
  );
  assert.equal(f.title, "Run a command · one container");
  assert.deepEqual(codeSpans("Runs (`pct exec`), unless `a = b` is set."), [
    "Runs (",
    { code: "pct exec" },
    "), unless ",
    { code: "a = b" },
    " is set.",
  ]);
});

// redesign-final-gen-e (review M1's counter half, the generic check's one
// instance): Stacks' "Problems 0" stood beside a red "does not build" or
// "will be destroyed" chip on every card. A stack with a red chip is a
// problem, so the counter and the chips always agree.
test("redesign-final-gen-e: a stack whose card carries a red chip counts as a problem", async () => {
  const { stackRows, onlyCounts } = await import("../js/stacksview.js");
  const NOW = 2_000_000;
  const st = (/** @type {string} */ name) => ({
    name,
    vmid: 100,
    online: true,
    enabled: true,
    apps_running: 1,
    apps_total: 1,
    env_sealed: true,
    apps: [],
  });
  const rows = stackRows(
    /** @type {any} */ ({ stacks: [st("a"), st("b"), st("c")] }),
    {
      drift: {
        a: { state: "not_compared", why: "latch_files is set" },
        b: { state: "no_local_files" },
        c: { state: "same" },
      },
      calendar: {
        stacks: { a: [NOW - 3600], b: [NOW - 3600], c: [NOW - 3600] },
      },
      now: NOW,
    },
  );
  for (const r of rows) {
    const red = r.flags.some((f) => f.tone === "bad");
    assert.equal(
      r.problem,
      red,
      `${r.name}: problem ${r.problem}, red chip ${red}`,
    );
  }
  assert.equal(onlyCounts(rows).problems, 2);
});
