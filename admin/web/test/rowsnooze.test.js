import assert from "node:assert/strict";
import { test } from "node:test";
import { at } from "./support/clock.js";
import {
  inboxNow,
  setSnoozes,
  snoozedNow,
  wireInbox,
} from "../js/inboxlist.js";

const NOW = at("2026-10-20T10:00:00Z");
/** @param {number} id @returns {import("../js/notices.js").Notice} */
const notice = (id) => ({
  id,
  at: NOW - 60,
  kind: /** @type {const} */ ("alert"),
  stack: "media",
  title: "media does not answer",
  body: "",
  read: false,
  level: /** @type {const} */ ("critical"),
  push: { state: /** @type {const} */ ("sent") },
});

test("a snoozed row leaves the list and the counter until its moment", (t) => {
  t.mock.timers.enable({ apis: ["Date"], now: NOW * 1000 });
  wireInbox(() => ({ asks: [], notices: [notice(1)] }));
  assert.equal(inboxNow().items.length, 1);
  const key = inboxNow().items[0].snoozeKey ?? "";
  assert.match(key, /^notice-group:/);
  setSnoozes([
    { key, until: NOW + 600, title: "t", remind: false, set_at: NOW },
  ]);
  assert.equal(inboxNow().items.length, 0);
  assert.equal(snoozedNow().length, 1);
  // The same thing said again is the same row: it stays snoozed.
  wireInbox(() => ({ asks: [], notices: [notice(1), notice(2)] }));
  assert.equal(inboxNow().items.length, 0);
  // An ended snooze brings it back.
  setSnoozes([{ key, until: NOW - 1, title: "t", remind: false, set_at: NOW }]);
  assert.equal(inboxNow().items.length, 1);
  assert.equal(snoozedNow().length, 0);
});
