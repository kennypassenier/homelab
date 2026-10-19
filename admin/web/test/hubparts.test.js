// feat-kp11demo-13 (the approved hub demo): checks as kp status rows with the
// fix written out, problems with their command, dependencies in kp's shape.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  commandFor,
  depItems,
  fixFor,
  passText,
  problemItems,
  resourcesSpec,
  statusRows,
} from "../js/hubparts.js";

test("feat-kp11demo-13: a failed or reserved check carries its fix, an unknown one is empty, never green", () => {
  const rows = statusRows([
    {
      key: "drift",
      text: "Matches its files",
      tone: "warn",
      verdict: "needs you",
    },
    { key: "disk", text: "Disk below 80%", tone: "bad", verdict: "failed" },
    { key: "web", text: "Web", tone: "unknown", verdict: "not measured" },
    { key: "online", text: "Up", tone: "ok", verdict: "ok" },
  ]);
  assert.deepEqual(
    rows.map((r) => r.state),
    ["warn", "failed", "empty", "ok"],
  );
  assert.match(rows[0].fix ?? "", /Deploy/);
  assert.equal(rows[3].fix, undefined);
  assert.match(fixFor("manual:x"), /Answer/);
  assert.equal(passText({ ok: 5, total: 7 }), "5 of 7 checks pass");
});

test("feat-kp11demo-13: a problem keeps its fix button, its remedy and the command that fixes it", () => {
  const items = problemItems(
    [
      {
        key: "drift",
        tone: "warn",
        title: "Its files changed",
        text: "Deploy makes it match",
        act: {
          kind: "none",
          label: "Deploy…",
          title: "Make the container match its files",
        },
      },
    ],
    "immich",
    () => null,
  );
  assert.equal(items[0].command, "homelab deploy immich");
  assert.equal(items[0].severity, "warning");
  assert.equal(items[0].remedy, "Make the container match its files");
  assert.equal(commandFor("offline", "x"), undefined);
});

test("feat-kp11demo-13: depends on and used by come from the edges, a route points the other way, a named address is none", () => {
  const edges = [
    { from: "immich", to: "postgres", kind: "declared" },
    { from: "gateway", to: "immich", kind: "route" },
    { from: "immich", to: "10.0.0.5", kind: "named" },
    { from: "other", to: "postgres", kind: "declared" },
  ];
  const got = depItems(edges, "immich", () => "running");
  assert.deepEqual(
    got.map((d) => `${d.from}>${d.to}`).sort(),
    ["immich>gateway", "immich>postgres"].sort(),
  );
});

test("feat-kp11demo-13: Resources shows memory as a share of the limit and marks every deploy", () => {
  const body = {
    panels: [
      {
        panel: { title: "CPU (whole container)" },
        series: [{ points: [[1, 10]] }],
      },
      {
        panel: { title: "Memory used (whole container)" },
        series: [{ points: [[1, 512 * 1024 * 1024]] }],
      },
      { panel: { title: "CPU per app" }, series: [{ points: [[1, 99]] }] },
    ],
  };
  const spec = resourcesSpec(
    body,
    [{ at: 5, label: "Deployed by Kenny" }],
    { from: 0, to: 10 },
    1024,
  );
  assert.deepEqual(
    spec.series.map((s) => s.label),
    ["CPU", "Memory"],
  );
  assert.equal(spec.series[1].points[0][1], 50);
  assert.equal(spec.annotations?.[0].label, "Deployed by Kenny");
});
