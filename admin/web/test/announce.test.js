// Live view: announce, plan and pause (Kenny, 2026-09-29). The bar's view
// model: what it says, what its countdown shows, which buttons it offers,
// the plan beside the page, and the element a step is marked on.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  announceView,
  isActive,
  jobRunning,
  leftNow,
  plan,
  planList,
  stillDriving,
  targetOf,
} from "../js/driveview.js";

/** @typedef {import("../js/driveview.js").DriveState} DriveState */

/** @param {Partial<DriveState>} [o] @returns {DriveState} */
const state = (o = {}) => ({
  active: true,
  by: "wsl",
  seq: 4,
  page: "/",
  form: null,
  last_at: 1000,
  idle_s: 600,
  announce: null,
  paused_by: null,
  stopped_by: null,
  plan: null,
  ...o,
});

const announce = (left = 3000, countdown = true) => ({
  id: 1,
  step: { do: "goto", path: "/jobs" },
  text: "go to the jobs page",
  countdown,
  total_ms: countdown ? 3000 : 0,
  left_ms: countdown ? left : 0,
});

test("follow_live_the_bar_says_the_next_step_and_counts_down", () => {
  assert.equal(announceView(state(), 0), null);
  assert.equal(announceView(null, 0), null);
  const s = state({ announce: announce() });
  const v = announceView(s, leftNow(s, 0));
  assert.deepEqual(
    {
      text: v?.text,
      count: v?.count,
      fraction: v?.fraction,
      paused: v?.paused,
    },
    {
      text: "Next: go to the jobs page",
      count: "3",
      fraction: 1,
      paused: false,
    },
  );
  // The digit follows the tab's own clock, and stops at 0.
  assert.equal(announceView(s, leftNow(s, 1200))?.count, "2");
  assert.equal(announceView(s, leftNow(s, 2500))?.count, "1");
  assert.equal(announceView(s, leftNow(s, 9000))?.count, "0");
  assert.equal(announceView(s, leftNow(s, 1500))?.fraction, 0.5);
});

test("follow_live_a_pause_freezes_the_countdown_and_offers_continue", () => {
  const s = state({ announce: announce(1800), paused_by: "kenny" });
  assert.equal(leftNow(s, 60_000), 1800, "frozen while paused");
  const v = announceView(s, leftNow(s, 60_000));
  assert.equal(v?.count, "2");
  assert.equal(v?.status, "Paused by kenny");
  assert.equal(v?.paused, true);
  // Paused between two steps: the bar stays, with nothing announced yet.
  const between = announceView(state({ paused_by: "kenny" }), 0);
  assert.equal(between?.text, "Next: Claude's next step");
  assert.equal(between?.countdown, false);
});

// fix-185: the bar carries the `seq` it was drawn against, so a Pause,
// Continue or Stop click can tell the server which round it means — a
// stale press (delayed, or a duplicate) that arrives once the state has
// moved on is then refused instead of landing on a later, unrelated round.
test("fix_185_the_bar_carries_the_seq_it_was_drawn_against", () => {
  const s = state({ seq: 7, announce: announce() });
  assert.equal(announceView(s, leftNow(s, 0))?.seq, 7);
  const later = state({ seq: 12, paused_by: "kenny" });
  assert.equal(announceView(later, 0)?.seq, 12);
});

test("follow_live_pause_and_stop_stay_while_a_driven_job_runs", () => {
  // Kenny, 2026-09-30: during `confirm --wait` only "Leave live view" was
  // left, because nothing was announced while the deploy ran.
  const form = {
    id: "f",
    action: "deploy",
    stack: "kp-soft",
    title: "Deploy · kp-soft",
    steps: ["review"],
    step: "review",
    step_index: 0,
    values: {},
    errors: {},
    run_error: null,
    job: { job: 7, state: "running", message: null, progress: null },
    fields: [],
    buttons: [],
  };
  const s = state({ form });
  assert.equal(announceView(s, 0), null, "not driving: no bar");
  const v = announceView(s, 0, true);
  assert.equal(v?.text, "Running: job 7 running");
  assert.equal(v?.paused, false);
  assert.equal(v?.countdown, false);
  assert.equal(
    announceView(state(), 0, true)?.text,
    "Next: Claude's next step",
  );
});

test("fix_173_pause_says_honestly_that_a_running_job_finishes_on_its_own", () => {
  // Kenny, 2026-10-02 06:55: Pause during a driven batch looked like it
  // had frozen everything ("Next: Claude's next step"), which Pause never
  // could do to a job already running on the host — it only holds the
  // queue before the NEXT job (fix-172). The bar must say so honestly
  // instead of implying the running job itself stopped.
  const form = {
    id: "f",
    action: "deploy",
    stack: "kp-soft",
    title: "Deploy · kp-soft",
    steps: ["review"],
    step: "review",
    step_index: 0,
    values: {},
    errors: {},
    run_error: null,
    job: { job: 7, state: "running", message: null, progress: null },
    fields: [],
    buttons: [],
  };
  const paused = announceView(state({ form, paused_by: "kenny" }), 0, true);
  assert.equal(
    paused?.text,
    "Running: job 7 running — this step cannot be paused once it started; it finishes on its own",
  );
  assert.equal(paused?.paused, true);
});

test("fix_173_pause_on_a_driven_batch_names_which_stack_still_runs", () => {
  const form = {
    id: "f",
    action: "batch",
    stack: "media,home",
    title: "Batch",
    steps: ["review"],
    step: "review",
    step_index: 0,
    values: {},
    errors: {},
    run_error: null,
    job: null,
    fields: [],
    buttons: [],
    edit: { family: "batch", guarded: 0, result: { batch: 42 } },
  };
  const running = announceView(state({ form }), 0, true);
  assert.equal(running?.text, "Running: batch 42");
  const paused = announceView(state({ form, paused_by: "kenny" }), 0, true);
  assert.equal(
    paused?.text,
    "Running: batch 42 — the stack running now finishes on its own; Pause holds the one after it",
  );
});

test("follow_live_typing_is_held_without_a_countdown", () => {
  const s = state({
    announce: { ...announce(0, false), text: "type into Snapshot" },
    paused_by: "kenny",
  });
  const v = announceView(s, leftNow(s, 0));
  assert.equal(v?.count, "");
  assert.equal(v?.countdown, false);
  assert.equal(v?.text, "Next: type into Snapshot");
});

test("follow_live_the_plan_marks_done_current_and_changed", () => {
  const s = state({
    plan: {
      by: "wsl",
      next: 1,
      changed: true,
      steps: [
        { step: { do: "goto" }, text: "go to the jobs page", done: true },
        { step: { do: "open" }, text: "open Deploy · media", done: false },
        { step: { do: "press" }, text: "press Deploy", done: false },
      ],
    },
  });
  const p = planList(s);
  assert.deepEqual(
    p?.items.map((x) => x.mark),
    ["done", "current", "todo"],
  );
  assert.equal(p?.counter, "Step 2 of 3");
  assert.equal(p?.changed, true);
  assert.equal(planList(state()), null);
  assert.equal(
    announceView(state({ ...s, announce: announce() }), 0)?.counter,
    "Step 2 of 3",
  );
});

test("follow_live_a_step_is_marked_on_its_target", () => {
  assert.deepEqual(targetOf({ do: "goto", path: "/jobs" }), {
    kind: "link",
    path: "/jobs",
  });
  assert.deepEqual(targetOf({ do: "open", form: "deploy", target: "media" }), {
    kind: "action",
    action: "deploy",
  });
  assert.deepEqual(
    targetOf({ do: "open", form: "firewall", target: "admin" }),
    { kind: "link", path: "/stacks/admin/firewall" },
  );
  assert.deepEqual(targetOf({ do: "pick", field: "act-app" }), {
    kind: "field",
    id: "act-app",
  });
  assert.deepEqual(targetOf({ do: "press", button: "confirm" }), {
    kind: "button",
    button: "confirm",
  });
  assert.deepEqual(targetOf({ do: "close" }), { kind: "close" });
  assert.deepEqual(targetOf({ do: "done" }), { kind: "none" });
});

test("follow_live_an_announcement_marks_but_is_no_step", () => {
  const local = { seq: 4, page: "/", form: null };
  const s = state({ announce: announce() });
  const ev = /** @type {import("../js/driveview.js").DriveEvent} */ ({
    kind: "announce",
    seq: 4,
    step: s.announce?.step,
    applied: false,
    refusal: null,
    state: s,
  });
  const p = plan(local, ev, true);
  assert.deepEqual(p.ops, [
    { op: "highlight", step: { do: "goto", path: "/jobs" } },
  ]);
  assert.equal(p.local, local);
  assert.deepEqual(plan(local, { ...ev, kind: "control" }, true).ops, []);
  assert.deepEqual(
    plan(local, ev, false).ops,
    [],
    "a tab that does not follow",
  );
});

// fix-188 (Kenny, 2026-10-02): driving a running Apply, Pause and Stop
// vanished partway through. `isActive`'s recency window is built to
// notice an abandoned session, not a job still working — a long host
// operation announces nothing while it runs, so `last_at` goes stale and
// `isActive` alone timed out mid-job. `stillDriving` is the one source
// drive.js now asks, so Pause/Stop never disappear while a job or a
// driven batch is actually running, however long it takes.
test("fix_188_pause_and_stop_stay_up_through_a_long_running_job", () => {
  /** @type {NonNullable<DriveState["form"]>} */
  const applyForm = {
    id: "action:apply",
    action: "apply",
    stack: "_host",
    title: "Apply · the whole host",
    steps: ["review"],
    step: "review",
    step_index: 0,
    values: {},
    errors: {},
    run_error: null,
    job: { job: 9, state: "running", message: null, progress: null },
    fields: [],
    buttons: [],
  };
  const stale = state({ last_at: 0, idle_s: 30, form: applyForm });
  // The recency check alone says Claude stopped driving long ago...
  assert.equal(isActive(stale, 10_000), false);
  // ...but a job is still running under this very state.
  assert.equal(jobRunning(stale), true);
  assert.equal(stillDriving(stale, 10_000), true);
  const v = announceView(stale, 0, stillDriving(stale, 10_000));
  assert.notEqual(v, null, "the strip (and its Pause/Stop) stays rendered");
  assert.equal(v?.text, "Running: job 9 running");

  // A driven batch, same shape, under `form.edit.result.batch`.
  const batchStale = state({
    last_at: 0,
    idle_s: 30,
    form: {
      ...applyForm,
      job: null,
      edit: { family: "batch", result: { batch: 4 }, guarded: 0 },
    },
  });
  assert.equal(isActive(batchStale, 10_000), false);
  assert.equal(jobRunning(batchStale), true);
  assert.notEqual(
    announceView(batchStale, 0, stillDriving(batchStale, 10_000)),
    null,
  );

  // Once the job and the batch are both gone and nothing is active or
  // announced, the strip is genuinely idle — Pause/Stop rightly disappear.
  const idle = state({ last_at: 0, idle_s: 30, form: null });
  assert.equal(stillDriving(idle, 10_000), false);
  assert.equal(announceView(idle, 0, stillDriving(idle, 10_000)), null);
});
