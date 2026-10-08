// The senior review of redesign-371-metrics / redesign-371-map (2026-10-03):
// one test per finding whose fix is pure logic. Each module is imported
// inside its test, so a missing function fails that test alone.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("review finding 4: Disk growth keeps one order, soonest full first, then fullest, then name", async () => {
  const { sortGrowth } = await import("../js/mapview.js");
  assert.equal(typeof sortGrowth, "function", "mapview.js has no sortGrowth");
  /** @param {string} subject @param {number | null} days @param {number} pct */
  const row = (subject, days, pct) => ({
    scope: "stack",
    subject,
    fit: { days_to_full: days, pct_now: pct, pct_per_day_robust: 0.1 },
  });
  const read = [
    row("c", null, 90),
    row("b", 40, 50),
    row("a", 40, 50),
    row("d", 12, 30),
    row("e", null, 95),
    row("f", 40, 70),
  ];
  const order = (/** @type {any[]} */ rows) =>
    sortGrowth(rows).map((r) => r.subject);
  assert.deepEqual(order(read), ["d", "f", "a", "b", "e", "c"]);
  // The same rows in any other order come out the same.
  assert.deepEqual(order([...read].reverse()), ["d", "f", "a", "b", "e", "c"]);
  // The read itself is left alone.
  assert.equal(read[0].subject, "c");
});

test("review finding 13: Stale images list major jumps, then what you can update, then the homelab binary's pins, by name", async () => {
  const { staleGroups } = await import("../js/mapview.js");
  const img = (
    /** @type {string} */ where_,
    /** @type {string | null} */ key,
    /** @type {string} */ pinned,
    /** @type {string} */ latest,
  ) => ({
    where_,
    key,
    pinned,
    latest,
    upstream: `github.com/example/${where_.split("/")[1]}`,
  });
  const g = staleGroups([
    img("admin/zeta", null, "v0.1.0", "v0.2.0"),
    img("web/beta", "web/beta", "1.4.2", "1.5.0"),
    img("api/omega", "api/omega", "v2.3.0", "v3.0.0"),
    img("web/alpha", "web/alpha", "1.0.0", "1.1.0"),
    img("ops/gamma", null, "v1.0.0", "v2.0.0"),
  ]);
  assert.deepEqual(
    g.map((x) => x.image),
    ["omega", "gamma", "alpha", "beta", "zeta"],
  );
});

test("review finding 12: the hostname legend's totals are the Hostnames table's numbers", async () => {
  const { hostLegend } = await import("../js/metricsview.js");
  assert.equal(
    typeof hostLegend,
    "function",
    "metricsview.js has no hostLegend",
  );
  /** @type {[number, number][]} */
  const pts = [
    [1, 10.4],
    [2, 20.4],
    [3, 30.4],
  ];
  const legend = hostLegend(
    [
      { label: "a.example.org", points: pts },
      { label: "", points: pts },
      { label: "z.example.org", points: pts },
    ],
    [
      ["a.example.org", 26839],
      ["", 412],
    ],
  );
  assert.equal(legend[0].n, 26839, "the table's number, not the points' sum");
  assert.equal(legend[1].label, "no hostname");
  assert.equal(legend[1].n, 412);
  // Not in the table (past its top 20): its own points' sum.
  assert.equal(legend[2].n, 61);
});

test("review finding 9: a card keeps one datatable stop, called before its table is replaced", async () => {
  const { tableSlot } = await import("../js/ui.js");
  assert.equal(typeof tableSlot, "function", "ui.js has no tableSlot");
  let live = 0;
  let attached = 0;
  const slot = tableSlot(() => {
    live += 1;
    attached += 1;
    return () => {
      live -= 1;
    };
  });
  const body = /** @type {any} */ ({});
  for (let i = 0; i < 2880; i += 1) slot.attach(body); // a day of 30 s reads
  assert.equal(attached, 2880);
  assert.equal(live, 1, "only the table on screen keeps its listeners");
  slot.stop();
  assert.equal(live, 0);
});

test("review finding 5: Metrics and the Map draw with ui.js, no page kit of their own", () => {
  // The 3.71.0 integration moved metricskit.js into ui.js (kpiStrip's
  // ctxParts / target / per-tile drive, tableSlot, dataTable) and deleted it.
  const url = (/** @type {string} */ f) =>
    new URL(`../js/pages/${f}`, import.meta.url);
  assert.throws(() => readFileSync(url("metricskit.js")), /ENOENT/);
  for (const f of ["metrics.js", "fleetview.js"]) {
    const src = readFileSync(url(f), "utf8");
    assert.doesNotMatch(src, /from "\.\/(metrics|host)kit\.js"/, f);
    for (const copy of [
      /function ensureStyle\b/,
      /function pageHeader\b/,
      /function kpi\(/,
      /function segmented\b/,
      /createElement\("link"\)/,
    ])
      assert.doesNotMatch(src, copy, `${f} holds its own ${copy}`);
    assert.match(src, /\bkpiStrip,[\s\S]*\} from "\.\.\/ui\.js"/, f);
  }
});

test("review finding 10: the chart's source, Show all and zoom Reset are declared Live view controls", async () => {
  await import("../js/charts.js");
  const { control } = await import("../js/drivable.js");
  for (const id of ["chart-source", "chart-show-all", "chart-zoom-reset"])
    assert.ok(control(id), `${id} is not declared`);
  assert.equal(control("chart-source")?.row, "<card>/<label>");
});

test("review finding 15: from the sixth source on, a chart's line is dashed", async () => {
  // kp-themes' own chart.js owns dashOf now (js/charts.js re-exports it):
  // 0 = solid for the first five, a dash index (1-3) once the colours come
  // round again.
  const { dashOf } = await import("../js/charts.js");
  assert.equal(typeof dashOf, "function", "charts.js has no dashOf");
  for (let i = 0; i < 5; i += 1) assert.equal(dashOf(i), 0);
  assert.ok(dashOf(5), "the sixth source repeats colour 1 without a dash");
  assert.notEqual(dashOf(5), dashOf(10));
});
