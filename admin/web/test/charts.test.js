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
import { PAD, layout, seriesHues } from "../js/charts.js";

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

// fix-253 (design review, 2026-10-03): a small count range (load average
// 0 to 1.3) labelled its y axis "1, 1, 0" — every tick label must differ.
test("layout's y tick labels are distinct, even for a small count range", () => {
  for (const [unit, hi] of /** @type {const} */ ([
    ["count", 1.3],
    ["count", 1],
    ["count", 0.4],
    ["count", 3],
    ["cores", 1.5],
    ["percent", 0.2],
  ])) {
    const L = layout(
      [
        {
          label: "x",
          points: [
            [0, 0],
            [10, hi],
          ],
        },
      ],
      0,
      10,
      unit,
    );
    const labels = L.yTicks.map((t) => t.label);
    assert.equal(
      new Set(labels).size,
      labels.length,
      `${unit} up to ${hi}: ${labels.join(", ")}`,
    );
  }
});

// fix-253: one reading per series still gives a point the page can draw.
test("layout gives a lone reading a point to draw, not an empty path", () => {
  const L = layout([{ label: "x", points: [[5, 1]] }], 0, 10, "count");
  assert.equal(L.paths[0].dots.length, 1);
  assert.ok(Number.isFinite(L.paths[0].dots[0].x));
});

// fix-252 (design review, 2026-10-03): the plot is laid out at the width it
// is shown at, so a phone's 11 px labels are not scaled down to 5 px.
test("layout lays the plot out at the width it is given", () => {
  const L = layout(
    [
      {
        label: "x",
        points: [
          [0, 0],
          [10, 1],
        ],
      },
    ],
    0,
    10,
    "count",
    340,
  );
  assert.equal(L.x1, 340 - PAD.right);
});
