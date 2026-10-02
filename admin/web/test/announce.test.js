// Live view: announce, plan and pause (Kenny, 2026-09-29). The bar's view
// model: what it says, what its countdown shows, which buttons it offers,
// the plan beside the page, and the element a step is marked on.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  announceView,
  leftNow,
  plan,
  planList,
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
