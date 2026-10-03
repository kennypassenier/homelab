// feat-shell-4 (redesign 3.71.0, decision "Inbox", Kenny 2026-10-03): one
// list of everything waiting for a person, worst first; the counter in the
// bar is its length, always the exact number (never "9+").
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  countText,
  inboxItems,
  inboxNow,
  onInbox,
  setInboxSource,
  wireInbox,
  worst,
} from "../js/inbox.js";

/** @param {Partial<import("../js/notices.js").Notice>} o */
const notice = (o) =>
  /** @type {import("../js/notices.js").Notice} */ ({
    id: 1,
    at: 100,
    kind: "action_failed",
    title: "Deploy failed",
    body: "it broke",
    read: false,
    push: { state: "sent" },
    ...o,
  });

test("feat-shell-4: counters are exact, never capped", () => {
  assert.equal(countText(0), "");
  assert.equal(countText(9), "9");
  assert.equal(countText(10), "10");
  assert.equal(countText(12), "12");
  assert.equal(countText(1234), "1234");
  assert.equal(countText(-3), "");
});

test("feat-shell-4: the Inbox holds open questions and unread notices that need a person, worst first", () => {
  const items = inboxItems({
    now: 1000,
    asks: [
      {
        id: 7,
        boot: "b",
        op: "update-kp",
        step: "restart",
        what: "restart crowdsec?",
        if_allowed: "it restarts",
        if_stopped: "it stays",
        asked_at: 990,
        deadline: 1200,
      },
      {
        id: 8,
        boot: "b",
        op: "old",
        step: "x",
        what: "gone",
        if_allowed: "",
        if_stopped: "",
        asked_at: 1,
        deadline: 2,
      },
    ],
    notices: [
      notice({ id: 1, level: "warning", at: 200 }),
      notice({ id: 2, level: "critical", kind: "alert", at: 100 }),
      notice({ id: 3, kind: "action_done", level: "info" }),
      notice({ id: 4, read: true }),
      notice({ id: 5, level: "ok", kind: "fleet_check" }),
      notice({ id: 6, level: "info", kind: "host_event", at: 300 }),
    ],
  });
  assert.deepEqual(
    items.map((i) => i.key),
    ["ask:b:7", "notice:2", "notice:1", "notice:6"],
  );
  assert.deepEqual(
    items.map((i) => i.severity),
    ["bad", "bad", "warn", "info"],
  );
  assert.equal(worst(items), "bad");
  assert.equal(worst([]), null);
});

test("feat-shell-4: a page's slower source joins the same list, so the count is the rows", () => {
  /** @type {number[]} */
  const counts = [];
  wireInbox(() => ({ asks: [], notices: [notice({ id: 1 })] }));
  const off = onInbox(() => counts.push(inboxNow().items.length));
  setInboxSource("stale-images", [
    {
      key: "stale:kp-soft",
      severity: "warn",
      title: "kp-soft has a newer version",
      why: "",
      href: "/inbox",
      stack: "kp-soft",
      at: 1,
      source: "x",
    },
  ]);
  const now = inboxNow();
  assert.equal(now.ready, true);
  assert.equal(now.items.length, 2);
  assert.equal(now.items[1].source, "stale-images");
  setInboxSource("stale-images", null);
  assert.equal(inboxNow().items.length, 1);
  off();
  assert.deepEqual(counts, [2, 1]);
  wireInbox(() => ({ asks: [], notices: null }));
  assert.equal(inboxNow().ready, false);
});
