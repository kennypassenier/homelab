// redesign-3.71 secrets: the Secrets page's pure half (secretsview.js).
import { test } from "node:test";
import assert from "node:assert/strict";

import {
  secretRows,
  splitReason,
  stackCount,
  stackKind,
  stackLine,
} from "../js/secretsview.js";
import { historyRows } from "../js/activity.js";
import { restartItems, usesOf } from "../js/pages/secrets.js";

test("feat-kp11demo-6: what restarts and which file feeds which app", () => {
  const d = {
    secrets: ["traefik"],
    files: [
      { from: "a/a.env", dest: "/x/a.env", restarts: "traefik" },
      { from: "a/b.env", dest: "/x/b.env", restarts: null },
    ],
  };
  assert.deepEqual(restartItems("gateway", d), [
    { from: "a/a.env", to: "traefik", state: "unknown" },
  ]);
  const u = usesOf("gateway", d);
  assert.deepEqual(u.apps, ["traefik"]);
  assert.deepEqual(u.uses.get("traefik/.env"), ["traefik"]);
  assert.deepEqual(u.uses.get("a/b.env"), [], "a file nothing reads");
  assert.equal(u.files.find((f) => f.key === "a/b.env")?.ghost, true);
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
