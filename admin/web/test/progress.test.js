// The contract of js/progress.js, point by point (see the header there).
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  byteFigure,
  mergeBytes,
  progressLine,
  sizeText,
  stepStates,
  steadyEta,
} from "../js/progress.js";

const MB = 1024 * 1024;

test("1: steps keep their order and every state, with at most one running", () => {
  const out = stepStates([
    { id: "a", state: "ok" },
    { id: "b", state: "wait" },
    { id: "c", state: "run" },
    { id: "d", state: "run" },
    { id: "e", state: "bad" },
    { id: "f", state: "skip" },
  ]);
  assert.deepEqual(
    out.map((r) => r.id),
    ["a", "b", "c", "d", "e", "f"],
  );
  assert.deepEqual(
    out.map((r) => r.state),
    ["done", "pending", "running", "pending", "failed", "skipped"],
  );
  // a state the list already speaks passes through; an unknown one is pending
  assert.equal(stepStates([{ state: "done" }])[0].state, "done");
  assert.equal(stepStates([{ state: "weird" }])[0].state, "pending");
});

test("2: readings of different transfers add up and never flip or shrink", () => {
  let b = mergeBytes(null, { label: "snapshot", done: 10, total: 100 }, 2);
  b = mergeBytes(b, { label: "upload", done: 0, total: 50 }, 2);
  assert.deepEqual([b.done, b.total], [10, 150]);
  b = mergeBytes(b, { label: "snapshot", done: 40, total: 100 }, 2);
  b = mergeBytes(b, { label: "upload", done: 5, total: 50 }, 2);
  // an older, smaller reading of the same transfer cannot pull it back
  b = mergeBytes(b, { label: "snapshot", done: 30, total: 100 }, 2);
  assert.deepEqual([b.done, b.total], [45, 150]);
  // alternating readings leave the figure monotonic
  let last = 0;
  for (let i = 0; i < 20; i++) {
    b = mergeBytes(
      b,
      { label: i % 2 ? "a-app" : "b-app", done: i * 10, total: 1000 },
      3,
    );
    assert.ok(b.done >= last);
    last = b.done;
  }
  // a new step starts from nothing
  const next = mergeBytes(b, { label: "x", done: 1, total: 9 }, 4);
  assert.deepEqual([next.done, next.total], [1, 9]);
  // a transfer with no total yet makes the whole total unknown
  assert.equal(
    mergeBytes(null, { label: "x", done: 5, total: null }, 1).total,
    null,
  );
});

test("3: a figure that is complete from its first reading claims no progress", () => {
  assert.equal(byteFigure(null), null);
  const done = byteFigure({ done: 77.8 * MB, total: 77.8 * MB });
  assert.equal(done?.kind, "complete");
  assert.equal(done?.text, "78 MB moved");
  const moving = byteFigure({ done: 12.4 * MB, total: 77.8 * MB });
  assert.equal(moving?.kind, "moving");
  assert.equal(moving?.text, "12 MB of 78 MB");
  assert.equal(moving?.percent, 15);
  assert.equal(byteFigure({ done: 5, total: null })?.kind, "unknown");
  assert.equal(byteFigure({ done: 5, total: null })?.percent, null);
  // never 100 % while something is still to move
  assert.equal(byteFigure({ done: 999, total: 1000 })?.percent, 99);
});

test("4: sizes use one format, a dot for the decimal", () => {
  assert.equal(sizeText(900), "900 B");
  assert.equal(sizeText(4.6 * 1024), "4.6 KB");
  assert.equal(sizeText(7.78 * MB), "7.8 MB");
  assert.equal(sizeText(null), null);
  for (const n of [1500, 5 * MB, 7.7 * MB, 3 * 1024 * MB])
    assert.doesNotMatch(String(sizeText(n)), /,/);
});

test("5: the line leaves out what is not known", () => {
  assert.equal(
    progressLine({
      step: 2,
      steps: 5,
      bytes: { done: 12.4 * MB, total: 77.8 * MB },
      etaSeconds: 25,
    }),
    "Step 2 of 5 · 12 MB of 78 MB · about 25 s left",
  );
  assert.equal(progressLine({ step: 2, steps: 5 }), "Step 2 of 5");
  assert.equal(progressLine({}), "");
  assert.equal(progressLine({ etaSeconds: 150 }), "about 3 min left");
});

test("6: the time left never grows within a step", () => {
  const eta = steadyEta();
  assert.equal(eta(2, 25), 25);
  assert.equal(eta(2, 15), 15);
  assert.equal(eta(2, 8), 8);
  assert.equal(eta(2, 25), 8);
  assert.equal(eta(2, undefined), undefined);
  // a new step starts afresh
  assert.equal(eta(3, 40), 40);
});
