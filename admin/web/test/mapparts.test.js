// feat-kp11demo-2 (the approved Map side views): what a stack uses and who
// uses it, what would stop with it, the matrix cell and the projection.
import { test } from "node:test";
import assert from "node:assert/strict";
import { blastOf, matrixCell, projection, usesOf } from "../js/mapparts.js";

/** @type {any} */
const topo = {
  nodes: [],
  edges: [
    { from: "paperless", to: "postgres", kind: "declared", detail: [] },
    { from: "immich", to: "postgres", kind: "open", detail: [] },
    { from: "traefik", to: "immich", kind: "route", detail: [] },
    { from: "immich", to: "10.0.0.2", kind: "named", detail: [] },
  ],
};

test("feat-kp11demo-2: uses and used by, a gateway route making the backend used by the gateway", () => {
  assert.deepEqual(
    usesOf(topo, "immich")
      .uses.map((x) => x.other)
      .sort(),
    ["postgres", "traefik"],
  );
  assert.deepEqual(
    usesOf(topo, "postgres")
      .usedBy.map((x) => x.other)
      .sort(),
    ["immich", "paperless"],
  );
  assert.deepEqual(blastOf(topo, "traefik"), ["immich"]);
});

test("feat-kp11demo-2: the matrix shows the connection kind, and with Not enforced only open and named ones", () => {
  assert.equal(
    matrixCell(topo, "paperless", "postgres", false)?.text,
    "declared",
  );
  assert.equal(matrixCell(topo, "paperless", "postgres", true)?.tone, "none");
  assert.equal(matrixCell(topo, "immich", "postgres", true)?.text, "open");
  assert.equal(matrixCell(topo, "a", "a", false), undefined);
});

test("feat-kp11demo-2: the projection runs from now to 100 % on the day it fills, and is none when it does not grow", () => {
  const p = projection(
    { pct_now: 80, pct_per_day_robust: 2, days_to_full: 10 },
    1000,
  );
  assert.equal(p.points[1][1], 100);
  assert.equal(p.fullAt, 1000 + 10 * 86400);
  assert.deepEqual(
    projection({ pct_now: 31, pct_per_day_robust: 0, days_to_full: null }, 1)
      .points,
    [],
  );
});
