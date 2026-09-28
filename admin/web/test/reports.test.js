// feat-ops-1, feat-ops-3, feat-ops-7 and doctor: the report pages' rows.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  entryOutcome,
  historyRows,
  incidentRows,
  parseIncident,
} from "../js/activity.js";
import { checkAnswer, checkRows, extraField } from "../js/checks.js";
import { doctorRows, doctorSummary, routeError } from "../js/doctor.js";

test("an incident name splits into its moment and its operation", () => {
  assert.deepEqual(parseIncident("1790000000-deploy-media"), {
    at: 1790000000,
    op: "deploy-media",
  });
  assert.deepEqual(parseIncident("1787850150-zfs-replicate"), {
    at: 1787850150,
    op: "zfs-replicate",
  });
  assert.deepEqual(parseIncident("stray"), { at: null, op: "stray" });
  const rows = incidentRows(["100-a", "300-b", "200-c"]);
  assert.deepEqual(
    rows.map((r) => r.op),
    ["b", "c", "a"],
  );
});

/** @type {import("../js/activity.js").Entry[]} */
const entries = [
  {
    kind: "op",
    start: 100,
    end: 222,
    label: "deploy",
    subject: "deploy media",
    req: 1,
    ok: false,
    error: "step 'native units' failed",
    steps: [
      { step: "validate", start: 100, end: 101, changed: false },
      { step: "firewall", start: 101, end: 102, changed: true },
    ],
  },
  { kind: "phase", start: 300, end: 3900, name: "backup", count: 12 },
  {
    kind: "op",
    start: 200,
    end: 205,
    label: "self-update",
    ok: false,
    deferred: "an operation is running",
    steps: [],
  },
  {
    kind: "op",
    start: 400,
    end: 400,
    label: "status",
    ok: true,
    steps: [{ step: "read", start: 400, end: 400, changed: false }],
  },
];

test("history rows: newest first, human durations, honest outcomes", () => {
  const rows = historyRows(entries);
  assert.deepEqual(
    rows.map((r) => [r.what, r.duration, r.outcome.label, r.by]),
    [
      ["status", "0 s", "ok", "host"],
      ["nightly backup (12 jobs)", "1 h", "done", "nightly round"],
      ["self-update", "5 s", "deferred", "host"],
      ["deploy media", "2 min 2 s", "failed", "asked"],
    ],
  );
  assert.equal(rows[3].detail, "step 'native units' failed");
  assert.equal(rows[0].detail, "1 step, 0 changed");
  assert.equal(
    entryOutcome({
      kind: "op",
      start: 400,
      end: 0,
      label: "deploy",
      ok: false,
      steps: [],
    }).label,
    "running",
  );
});

const rec = {
  stack: "media",
  app: "jellyfin",
  text: "Play one film.",
  registered_at: 1000,
  note: "",
};

test("a manual check's answer: open, ok, not ok, or accepted for now", () => {
  assert.equal(checkAnswer({ ...rec, ok: null }, 5000).label, "open");
  assert.equal(checkAnswer({ ...rec, ok: true }, 5000).label, "ok");
  assert.equal(checkAnswer({ ...rec, ok: false }, 5000).label, "not ok");
  assert.equal(
    checkAnswer({ ...rec, ok: false, accepted_until: 6000 }, 5000).label,
    "accepted",
  );
  assert.equal(
    checkAnswer({ ...rec, ok: false, accepted_until: 4000 }, 5000).label,
    "not ok",
  );
});

test("check rows put open ones first and show unknown fields readably", () => {
  const rows = checkRows(
    [
      { id: "a", record: { ...rec, ok: true, answered_at: 2000 } },
      {
        id: "b",
        record: { ...rec, app: "sonarr", ok: null, answered_hash: "abc" },
      },
    ],
    5000,
    { locale: "en-GB", timeZone: "UTC" },
  );
  assert.deepEqual(
    rows.map((r) => r.id),
    ["b", "a"],
  );
  assert.equal(rows[0].extras, "id: b · answered hash: abc");
  assert.equal(rows[1].answered, 2000);
  assert.equal(rows[0].answered, null);
  assert.match(
    extraField("snoozed_until", 1790000000, {
      locale: "en-GB",
      timeZone: "UTC",
    }),
    /^snoozed until: 21 Sept? 2026, 14:13$/,
  );
});

test("doctor rows put failures first and count by health", () => {
  const report = {
    overall: "warn",
    checks: [
      { name: "disk", health: "ok", detail: "53% free" },
      { name: "offsite", health: "fail", detail: "no token", remedy: "set it" },
      { name: "drill", health: "warn", detail: "old", remedy: null },
    ],
  };
  assert.deepEqual(
    doctorRows(report).map((r) => [r.name, r.health.label, r.remedy]),
    [
      ["offsite", "fail", "set it"],
      ["drill", "warn", ""],
      ["disk", "ok", ""],
    ],
  );
  assert.equal(doctorSummary(report), "3 checks · 1 ok · 1 warn · 1 fail");
});

test("a route's error reads as what, why and fix", () => {
  assert.deepEqual(
    routeError("the doctor", 502, {
      what: "doctor",
      why: "timed out",
      fix: "check the host",
    }),
    { what: "doctor", why: "timed out", fix: "check the host" },
  );
  assert.equal(
    routeError("the doctor", 401, null).fix,
    "log in again at /login",
  );
  assert.match(routeError("x", 500, "oops").why, /HTTP 500/);
  assert.match(routeError("x", 0, null).why, /did not answer/);
});
