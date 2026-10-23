// one-progress-view-1 (Kenny, 2026-10-23): the logic behind the shared
// progress view: the job list keeps a run's progress across a re-read,
// the step list holds every step from the first frame in one fixed order,
// the figures show a total from the start, the time bar agrees with the
// list, and a part is redrawn only when its content changed.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  jobSteps,
  keepProgress,
  medianView,
  mergeJobLists,
  stepFigures,
  stepText,
  timeView,
  upsertJob,
} from "../js/jobs.js";
import { signatureGuard } from "../js/runview.js";

/** @param {Partial<import("../js/jobs.js").Job>} [o] @returns {import("../js/jobs.js").Job} */
const job = (o = {}) => ({
  job: 1,
  origin: { from: "manual" },
  stack: "admin",
  action: "deploy",
  args: {},
  state: "running",
  queued_at: 900,
  started_at: 1000,
  finished_at: null,
  reqs: [1],
  message: null,
  cli: null,
  progress: null,
  restarts_dashboard: false,
  ...o,
});
/** @param {Partial<import("../js/jobs.js").Progress>} [o] @returns {import("../js/jobs.js").Progress} */
const prog = (o = {}) => ({
  op: "deploy-admin",
  step: "pull",
  n: 15,
  m: 86,
  finished: false,
  changed: false,
  expected_step_s: null,
  expected_total_s: null,
  expected_remaining_s: null,
  elapsed_s: 0,
  runs: 0,
  ...o,
});

test("progress-stays-put-1: the 5 s re-read of the job list keeps the step progress the server's copy does not carry", () => {
  const held = [job({ progress: prog({ n: 15 }) })];
  // The server's copy has no progress: before the fix the list flipped to
  // "Starting" / "0 of —" until the next progress event.
  const merged = mergeJobLists([job()], held, new Set());
  assert.equal(merged[0].progress?.n, 15);
  assert.equal(stepFigures(merged[0]).done, 14);
  // Many re-reads in a row never drop it.
  let list = held;
  for (let i = 0; i < 5; i++) list = mergeJobLists([job()], list, new Set());
  assert.equal(list[0].progress?.n, 15);
  // upsertJob (the live channel) agrees.
  assert.equal(upsertJob(held, job())[0].progress?.n, 15);
});

test("progress-stays-put-1: progress only moves forward, an older snapshot never pulls a run back", () => {
  const ahead = job({ progress: prog({ n: 20 }) });
  const behind = job({ progress: prog({ n: 12 }) });
  assert.equal(keepProgress(behind, ahead).progress?.n, 20);
  assert.equal(keepProgress(ahead, behind).progress?.n, 20);
  // The same step, finished, beats the same step, running.
  const fin = job({ progress: prog({ n: 12, finished: true }) });
  assert.equal(keepProgress(behind, fin).progress?.finished, true);
});

test("progress-stays-put-1: every step is in the list from the first frame to the end, in one fixed order, never folded", () => {
  const names = new Map();
  /** @type {string[][]} */
  const frames = [];
  const total = 31;
  /** @param {import("../js/jobs.js").Job} j */
  const snap = (j) => frames.push(jobSteps(j, names).map((r) => r.title));
  snap(job({ progress: prog({ n: 0, m: total, step: "" }) }));
  for (let n = 1; n <= total; n++) {
    names.set(n, `step-${n}`);
    snap(job({ progress: prog({ n, m: total, step: `step-${n}` }) }));
  }
  snap(
    job({
      state: "done",
      progress: prog({ n: total, m: total, finished: true }),
    }),
  );
  for (const f of frames) assert.equal(f.length, total, "the row count moved");
  // Row i keeps its place: once named, a row never changes position.
  const last = frames[frames.length - 1];
  for (let i = 0; i < total; i++) assert.equal(last[i], `step-${i + 1}`);
  const mid = frames[10];
  for (let i = 0; i < 10; i++) assert.equal(mid[i], `step-${i + 1}`);
});

test("progress-stays-put-1: the phase before the first step is a state of the list, not a card replacing it", () => {
  const planned = jobSteps(
    job({ progress: prog({ n: 0, m: 5, step: "" }) }),
    new Map(),
  );
  assert.equal(planned.length, 5);
  assert.equal(planned[0].state, "run");
  assert.match(planned[0].desc, /getting ready/);
  assert.ok(planned.slice(1).every((r) => r.state === "wait"));
  assert.equal(
    stepText(job({ progress: prog({ n: 0, m: 5 }) })),
    "starting, 5 steps planned",
  );
});

test("progress-stays-put-1: a finished step reads done in the list and in the figures alike", () => {
  const j = job({ progress: prog({ n: 3, m: 5, finished: true }) });
  const rows = jobSteps(j, new Map());
  assert.deepEqual(
    rows.map((r) => r.state),
    ["ok", "ok", "ok", "wait", "wait"],
  );
  assert.equal(stepFigures(j).done, 3);
});

test("steps-total-known-1: the total shows from the first frame; a total that can grow says so in words", () => {
  const first = stepFigures(job({ progress: prog({ n: 0, m: 86 }) }));
  assert.deepEqual([first.done, first.of, first.note], [0, "86", ""]);
  const unknown = stepFigures(job({ progress: prog({ n: 3, m: null }) }));
  assert.equal(unknown.of, "at least 3");
  assert.match(unknown.note, /grows/);
  const none = stepFigures(job());
  assert.equal(none.of, "not counted yet");
  assert.match(none.note, /counts the steps/);
  // The list says so too: a trailing row, never a blank.
  const rows = jobSteps(job({ progress: prog({ n: 3, m: null }) }), new Map());
  assert.equal(rows[rows.length - 1].title, "More steps may follow");
  // A finished run has no "may follow" row and no note.
  const done = job({
    state: "done",
    progress: prog({ n: 3, m: null, finished: true }),
  });
  assert.equal(jobSteps(done, new Map()).length, 3);
  assert.equal(stepFigures(done).note, "");
});

test("timebar-after-done-1: the time bar never says 'no step has finished' while steps show done, and names the gap to the time taken", () => {
  const rows = [
    { title: "a", state: "ok" },
    { title: "b", state: "run" },
  ];
  const empty = timeView(rows, new Map(), null, 30, false);
  assert.equal(empty.kind, "empty");
  assert.doesNotMatch(
    /** @type {any} */ (empty).message,
    /No step has finished/,
  );
  const fresh = timeView(
    [{ title: "a", state: "run" }],
    new Map(),
    null,
    2,
    false,
  );
  assert.match(/** @type {any} */ (fresh).message, /No step has finished/);
  const bar = timeView(rows, new Map([["a", 28]]), 120, 64, true);
  assert.equal(bar.kind, "bar");
  assert.match(/** @type {any} */ (bar).note, /28 s of the 1 min 4 s/);
  const exact = timeView(rows, new Map([["a", 63]]), 120, 64, true);
  assert.equal(/** @type {any} */ (exact).note, "");
});

test("timebar-after-done-1: a median that measures a part of the job is named and draws no 'slower' badge", () => {
  const est = {
    median: 2,
    min: null,
    max: null,
    runs: 3,
    text: "median 2 s over 3 earlier runs",
  };
  const part = medianView(est, 64, "done", "install-native");
  assert.equal(part.comparable, false);
  assert.equal(part.delta, null);
  assert.match(part.text, /of install-native, a part of this job/);
  const same = medianView({ ...est, median: 60 }, 64, "done");
  assert.equal(same.comparable, true);
  assert.deepEqual(same.delta, { tone: "warning", text: "4 s slower" });
  assert.equal(medianView(null, 5, "done").text, "no median yet");
});

test("redraw-unchanged-1: a part is redrawn only when its content changed, so a finished panel draws nothing again", () => {
  const guard = signatureGuard();
  let draws = 0;
  const draw = () => void draws++;
  const hostSaid = { text: "exit 1", title: "What the host said" };
  assert.equal(guard("code", hostSaid, draw), true);
  // Ten refreshes of a finished job with the same content: zero redraws.
  for (let i = 0; i < 10; i++)
    guard("code", { text: "exit 1", title: "What the host said" }, draw);
  assert.equal(draws, 1);
  assert.equal(guard("code", { ...hostSaid, text: "exit 2" }, draw), true);
  assert.equal(draws, 2);
  // Parts are independent.
  assert.equal(guard("outcome", null, draw), true);
  assert.equal(guard("outcome", null, draw), false);
});
