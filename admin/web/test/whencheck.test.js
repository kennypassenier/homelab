// manual-checks-as-info-1 (Kenny, 2026-10-23): the checks only a person can
// confirm are information on a stack's page, never a notification. Pure
// logic only: the rows of "When something goes wrong, check this", and that
// no other part of the dashboard turns a check into attention.
import { test } from "node:test";
import assert from "node:assert/strict";
import { STANDING_STACK, whenToCheckRows } from "../js/checks.js";
import { attentionItems, healthChecks } from "../js/stackhub.js";
import * as rows from "../js/inboxrows.js";

const rec = (/** @type {object} */ o) => ({
  stack: "media",
  app: "jellyfin",
  text: "Does the picture look right?",
  registered_at: 10,
  ...o,
});

test("manual-checks-as-info-1: a stack's checks list who, where, and when it was last confirmed or never", () => {
  const list = whenToCheckRows(
    [
      {
        id: "b",
        record: rec({
          app: "sonarr",
          text: "Zeta",
          ok: true,
          answered_at: 500,
        }),
      },
      { id: "a", record: rec({ url: "https://jf.example/web", ok: null }) },
      { id: "x", record: rec({ stack: "other", text: "not mine" }) },
      {
        id: "n",
        record: rec({
          app: "sonarr",
          text: "Alpha",
          ok: false,
          answered_at: 700,
        }),
      },
    ],
    "media",
  );
  assert.deepEqual(
    list.map((r) => [r.id, r.url, r.confirmed, r.notOk]),
    [
      ["a", "https://jf.example/web", null, null],
      ["n", null, null, 700],
      ["b", null, 500, null],
    ],
    "sorted by app then text; never = null; not ok is not 'confirmed'",
  );
});

test("manual-checks-as-info-1: a link that is not http(s) is not offered", () => {
  const [r] = whenToCheckRows(
    [{ id: "a", record: rec({ url: "javascript:alert(1)" }) }],
    "media",
  );
  assert.equal(r.url, null);
});

test("manual-checks-as-info-1: the stack-less standing check is listed under 'standing'", () => {
  const list = whenToCheckRows(
    [
      {
        id: "p",
        record: rec({
          stack: STANDING_STACK,
          app: "backups",
          text: "password chain",
        }),
      },
      { id: "m", record: rec({}) },
    ],
    STANDING_STACK,
  );
  assert.deepEqual(
    list.map((r) => r.id),
    ["p"],
  );
});

test("manual-checks-as-info-1: the Inbox has no source for manual checks, and the stack hub raises nothing for them", () => {
  assert.equal("checkRowsOf" in rows, false);
  const stack = {
    name: "media",
    online: true,
    enabled: true,
    apps_total: 0,
    apps_running: 0,
    apps: [],
  };
  const items = attentionItems(
    /** @type {any} */ ({
      stack: "media",
      s: stack,
      now: 1_000_000,
      openChecks: 5,
    }),
  );
  assert.ok(items.every((p) => p.key !== "manual-checks"));
  const health = healthChecks(
    /** @type {any} */ ({
      s: stack,
      night: "ok",
      diskPct: 10,
      drift: null,
      manual: [
        {
          id: "c",
          app: "a",
          text: "t",
          answer: { label: "open", tone: "warn" },
        },
      ],
    }),
  );
  assert.ok(health.every((r) => !r.key.startsWith("manual:")));
});
