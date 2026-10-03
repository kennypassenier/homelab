// redesign-3.71 secrets: the Secrets page's pure half (secretsview.js).
import { mock, test } from "node:test";
import assert from "node:assert/strict";

import {
  REVEAL_MS,
  createReveals,
  secretRows,
  splitReason,
  stackCount,
  stackKind,
  stackLine,
} from "../js/secretsview.js";
import { historyRows } from "../js/activity.js";

test("redesign-371: a revealed value hides itself after 30 seconds", () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 0 });
  try {
    let changes = 0;
    const r = createReveals({ onChange: () => (changes += 1) });
    assert.equal(REVEAL_MS, 30_000);
    r.reveal("gateway/traefik/.env", "TOKEN=abc");
    assert.equal(r.value("gateway/traefik/.env"), "TOKEN=abc");
    assert.equal(r.left("gateway/traefik/.env"), 30_000);
    mock.timers.tick(29_999);
    assert.equal(r.value("gateway/traefik/.env"), "TOKEN=abc", "not yet");
    assert.equal(r.left("gateway/traefik/.env"), 1);
    mock.timers.tick(1);
    assert.equal(r.value("gateway/traefik/.env"), null, "hidden at 30 s");
    assert.equal(r.left("gateway/traefik/.env"), 0);
    assert.equal(r.size, 0);
    assert.equal(changes, 2, "shown, then hidden: the page repaints twice");
  } finally {
    mock.timers.reset();
  }
});

test("redesign-371: revealing again restarts the 30 seconds; Hide and Hide all act at once", () => {
  mock.timers.enable({ apis: ["setTimeout", "Date"], now: 0 });
  try {
    const r = createReveals({ onChange: () => {} });
    r.reveal("a", "1");
    mock.timers.tick(20_000);
    r.reveal("a", "1");
    mock.timers.tick(20_000);
    assert.equal(r.value("a"), "1", "the second reveal counts from itself");
    mock.timers.tick(10_000);
    assert.equal(r.value("a"), null);
    r.reveal("a", "1");
    r.reveal("b", "2");
    r.hide("a");
    assert.equal(r.value("a"), null);
    assert.equal(r.value("b"), "2");
    r.hideAll();
    assert.equal(r.size, 0);
    // A timer of a hidden value never fires into a later reveal.
    r.reveal("b", "3");
    mock.timers.tick(29_000);
    assert.equal(r.value("b"), "3");
    r.clear();
    assert.equal(r.value("b"), null);
  } finally {
    mock.timers.reset();
  }
});

test("redesign-371: the left pane's kinds, exact counts and lines", () => {
  const ok = { secrets: ["traefik", "cloudflared"], files: [] };
  assert.equal(stackKind(ok), "ok");
  assert.equal(stackCount(ok), 2);
  assert.equal(stackLine(ok), "2 secrets · 0 files");
  const one = {
    secrets: [],
    files: [{ from: "admin/admin.env", dest: "/appdata/admin.env" }],
  };
  assert.equal(stackLine(one), "0 secrets · 1 file");
  assert.equal(stackKind({ secrets: [], files: [] }), "none");
  assert.equal(stackKind(undefined), "none");
  assert.equal(stackLine(undefined), "declares none");
  const bad = { secrets: [], files: [], unreadable: "it broke :: fix it" };
  assert.equal(stackKind(bad), "unreadable");
  assert.equal(stackCount(bad), 0);
  assert.deepEqual(splitReason(bad.unreadable), {
    why: "it broke",
    fix: "fix it",
  });
  assert.deepEqual(splitReason("no fix given"), {
    why: "no fix given",
    fix: "",
  });
});

test("redesign-371: one row per secret, keyed by its latch path", () => {
  const rows = secretRows("admin", {
    secrets: ["web"],
    files: [{ from: "admin/admin.env", dest: "/appdata/admin.env" }],
  });
  assert.deepEqual(
    rows.map((r) => [r.key, r.name]),
    [
      ["admin/web/.env", "web/.env"],
      ["admin/admin/admin.env", "admin/admin.env"],
    ],
  );
  assert.deepEqual(rows[1].ref, {
    kind: "file",
    from: "admin/admin.env",
    dest: "/appdata/admin.env",
  });
});

test("redesign-371: Activity names who revealed or copied which secret, never a value", () => {
  const rows = historyRows([
    {
      kind: "op",
      start: 100,
      end: 100,
      label: "reveal-secret",
      subject: "revealed gateway/traefik/.env",
      req: 3,
      by: "Kenny",
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: 200,
      end: 200,
      label: "copy-secret",
      subject: "copied gateway/traefik/.env",
      req: 4,
      by: "Claude (Live view)",
      ok: true,
      steps: [],
    },
  ]);
  assert.deepEqual(
    rows.map((r) => [r.what, r.by, r.duration]),
    [
      [
        "Claude (Live view) copied gateway/traefik/.env",
        "Claude (Live view)",
        "—",
      ],
      ["Kenny revealed gateway/traefik/.env", "Kenny", "—"],
    ],
  );
});
