// Live view pace (Kenny, 2026-09-29 ~18:14): typing and dropdown picks
// read as a person, not a machine. Deterministic: the randomness is
// injected.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  PICK_CHOICE_PAUSE_MS,
  PICK_HOVER_MS,
  PICK_MAX_HOVERS,
  TYPE_FLOOR_MS,
  TYPE_MAX_MS,
  TYPE_MAX_TOTAL_MS,
  TYPE_MIN_MS,
  listTop,
  pickPath,
  pickPlan,
  typingDelays,
} from "../js/drivepace.js";

/** A fixed sequence, repeated. @param {number[]} xs */
const seq = (xs) => {
  let i = 0;
  return () => xs[i++ % xs.length];
};
const sum = (/** @type {number[]} */ xs) => xs.reduce((a, b) => a + b, 0);

test("typing waits 90 to 180 ms per letter, unevenly", () => {
  const d = typingDelays("abcdef", seq([0, 0.5, 0.999, 0.25, 0.75, 0.1]));
  assert.equal(d.length, 6);
  assert.deepEqual(d, [90, 135, 180, 113, 158, 99]);
  for (const x of typingDelays("letters", Math.random)) {
    assert.ok(x >= TYPE_MIN_MS && x <= TYPE_MAX_MS, `delay ${x}`);
  }
  // Not the same for every letter.
  assert.ok(new Set(d).size > 1);
});

test("typing rests after a word and longer after punctuation", () => {
  const mid = () => 0.5;
  const [a, space, b, comma, c] = typingDelays("a b,c", mid);
  assert.equal(a, 135);
  assert.equal(b, 135);
  assert.equal(c, 135);
  assert.equal(space, 135 + 110);
  assert.equal(comma, 135 + 270);
  assert.ok(comma > space && space > a);
});

test("a long text still finishes within the cap; reduced motion types at once", () => {
  const long = "x".repeat(400);
  const d = typingDelays(long, () => 0.5);
  assert.equal(d.length, 400);
  assert.ok(sum(d) <= TYPE_MAX_TOTAL_MS + 400, `total ${sum(d)}`);
  assert.ok(d.every((x) => x >= TYPE_FLOOR_MS));
  // A short text is not squeezed.
  assert.deepEqual(
    typingDelays("ab", () => 0),
    [90, 90],
  );
  assert.deepEqual(typingDelays("anything", Math.random, true), []);
  assert.deepEqual(typingDelays("", Math.random), []);
  // Letters, not UTF-16 units.
  assert.equal(typingDelays("é🙂", () => 0).length, 2);
});

test("a pick passes over the options on its way, the chosen one last", () => {
  assert.deepEqual(pickPath(0, 3), [1, 2, 3]);
  assert.deepEqual(pickPath(5, 2), [4, 3, 2]);
  // Nothing selected yet: from the top of the list.
  assert.deepEqual(pickPath(-1, 2), [0, 1, 2]);
  // Already on it: it rests there.
  assert.deepEqual(pickPath(2, 2), [2]);
  assert.deepEqual(pickPath(0, -1), []);
  // A long way keeps a few stops, evenly spread, ending on the choice.
  const far = pickPath(0, 40);
  assert.equal(far.length, PICK_MAX_HOVERS);
  assert.equal(far.at(-1), 40);
  assert.ok(far.every((x, i) => i === 0 || x > far[i - 1]));
  assert.deepEqual(pickPath(0, 6, 3), [2, 4, 6]);
});

test("the pick plan is slow and visible; reduced motion sets at once", () => {
  const p = pickPlan(0, 2, () => 0.5);
  assert.ok(p);
  assert.deepEqual(
    p.hovers.map((x) => x.index),
    [1, 2],
  );
  const hover = (PICK_HOVER_MS[0] + PICK_HOVER_MS[1]) / 2;
  assert.ok(p.hovers.every((x) => x.restMs === hover));
  assert.equal(p.choicePauseMs, PICK_CHOICE_PAUSE_MS);
  const total =
    p.glideMs +
    p.openPauseMs +
    sum(p.hovers.map((x) => x.moveMs + x.restMs)) +
    p.choicePauseMs +
    p.closeMs;
  // Much slower than the old instant set, and still under a few seconds.
  assert.ok(total >= 2000 && total <= 5000, `total ${total}`);
  assert.equal(
    pickPlan(0, 2, () => 0.5, true),
    null,
  );
  assert.equal(
    pickPlan(0, -1, () => 0.5),
    null,
  );
});

test("the drawn list opens below the dropdown, or above without room", () => {
  const band = { top: 80, bottom: 686 };
  assert.equal(listTop({ top: 200, height: 36 }, 150, band), 236);
  assert.equal(listTop({ top: 600, height: 36 }, 150, band), 450);
  // No room either way: the side with more.
  assert.equal(listTop({ top: 300, height: 36 }, 900, band), 336);
});
