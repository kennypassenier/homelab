// fix-220 (Kenny, 2026-10-02: "al die items zijn amper van elkaar te
// distinguieren"): `seriesHues` is charts.js's one pure, DOM-free piece —
// a deterministic hue per series, evenly spaced, the same technique
// topology.js's `stackHues` uses for its legend (see topology.test.js's own
// note on why this suite does not render `panelEl` itself: `node --test`
// has no DOM — the rendered page, the description paragraph, the health
// table and the humanized labels are pinned by the invariants e2e smoke
// against the demo host's own made-up Prometheus/Loki instead).
import { test } from "node:test";
import assert from "node:assert/strict";
import { seriesHues } from "../js/charts.js";

test("seriesHues spaces every series evenly around the wheel", () => {
  assert.deepEqual(seriesHues(1), [0]);
  assert.deepEqual(seriesHues(2), [0, 180]);
  assert.deepEqual(seriesHues(4), [0, 90, 180, 270]);
});

test("seriesHues of many series still gives that many distinct hues", () => {
  const hues = seriesHues(12);
  assert.equal(hues.length, 12);
  assert.equal(new Set(hues).size, 12);
});

test("seriesHues of zero series is empty, not a division error", () => {
  assert.deepEqual(seriesHues(0), []);
});
