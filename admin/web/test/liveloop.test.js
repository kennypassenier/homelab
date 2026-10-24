// fix-live-1: the live channel is reopened with a backoff of 1 s to 15 s,
// and logs.js's level filter (feat-logs-1) remembers its choice.
import { test } from "node:test";
import assert from "node:assert/strict";
import { reopenDelay } from "../js/store.js";
import {
  LOG_LEVELS_KEY,
  levelBucket,
  readLogLevels,
  saveLogLevels,
  wholeLogLines,
} from "../js/logs.js";
import { ACTION_TOAST_MS, toastMs } from "../js/ui.js";

test("the first reopen is immediate, then the wait doubles from 1 s and stops at 15 s", () => {
  assert.deepEqual(
    [0, 1, 2, 3, 4, 5, 9].map(reopenDelay),
    [0, 1000, 2000, 4000, 8000, 15000, 15000],
  );
});

test("an action toast stays at least 15 s, a plain one keeps its time", () => {
  assert.equal(toastMs(true, undefined), ACTION_TOAST_MS);
  assert.equal(toastMs(true, 6000), ACTION_TOAST_MS);
  assert.equal(toastMs(true, 30000), 30000);
  assert.equal(toastMs(false, undefined), 6000);
  assert.equal(toastMs(false, 2000), 2000);
});

test("levels fold into debug, info, warning and critical", () => {
  assert.equal(levelBucket("TRACE"), "debug");
  assert.equal(levelBucket("warn"), "warning");
  assert.equal(levelBucket("Warning"), "warning");
  assert.equal(levelBucket("err"), "critical");
  assert.equal(levelBucket("fatal"), "critical");
  assert.equal(levelBucket(""), "info");
  assert.equal(levelBucket("whatever"), "info");
});

test("the chosen levels are remembered, and a broken memory gives the defaults", () => {
  /** @type {Map<string, string>} */
  const m = new Map();
  const store = {
    getItem: (/** @type {string} */ k) => m.get(k) ?? null,
    setItem: (/** @type {string} */ k, /** @type {string} */ v) =>
      void m.set(k, v),
  };
  assert.deepEqual([...readLogLevels(store)], ["info", "warning", "critical"]);
  saveLogLevels(["debug", "critical"], store);
  assert.deepEqual([...readLogLevels(store)], ["debug", "critical"]);
  m.set(LOG_LEVELS_KEY, "{not json");
  assert.deepEqual([...readLogLevels(store)], ["info", "warning", "critical"]);
});

test("the whole-log answer has two shapes", () => {
  assert.equal(wholeLogLines({ lines: [1] })?.length, 1);
  assert.equal(wholeLogLines([1, 2])?.length, 2);
  assert.equal(wholeLogLines({ nope: 1 }), null);
});
