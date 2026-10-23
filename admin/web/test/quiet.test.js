import assert from "node:assert/strict";
import { test } from "node:test";
import { REFRESH_MS, REFRESH_S, changeGate, contentKey } from "../js/quiet.js";

test("the default refresh is one minute", () => {
  assert.equal(REFRESH_MS, 60_000);
  assert.equal(REFRESH_S, 60);
});

test("contentKey ignores key order and the ignored keys", () => {
  assert.equal(
    contentKey({ a: 1, b: { c: 2, d: [1, { y: 1, x: 2 }] }, t: 5 }, ["t"]),
    contentKey({ t: 9, b: { d: [1, { x: 2, y: 1 }], c: 2 }, a: 1 }, ["t"]),
  );
  assert.notEqual(contentKey({ a: 1 }), contentKey({ a: 2 }));
});

test("changeGate is new on the first read and when content moves only", () => {
  const g = changeGate(["read_at"]);
  assert.equal(g.seen(), false);
  assert.equal(g.isNew({ x: 1, read_at: 1 }), true);
  assert.equal(g.seen(), true);
  assert.equal(g.isNew({ read_at: 2, x: 1 }), false);
  assert.equal(g.isNew({ x: 2, read_at: 2 }), true);
  g.forget();
  assert.equal(g.isNew({ x: 2, read_at: 2 }), true);
});
