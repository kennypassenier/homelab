// fix-207 (Kenny, Dutch): "gebruik voor elke stack een andere kleur" —
// `stackHues` is topology.js's one pure, DOM-free piece: a deterministic
// hue per stack, evenly spaced in sorted order. The rest of fix-207 (the
// legend grid, the firewall ring, hover/focus isolation) only exists once
// real SVG elements are on a real page, so it is pinned against the actual
// rendered DOM by the invariants e2e smoke instead (docs/INVARIANTS.md) —
// this suite (`node --test`) has no DOM to render into.
import { test } from "node:test";
import assert from "node:assert/strict";
import { stackHues } from "../js/topology.js";

test("every stack gets its own hue, evenly spaced, in sorted order", () => {
  const nodes = [
    { stack: "zeta", vmid: 3, ip: "10.0.0.3" },
    { stack: "alpha", vmid: 1, ip: "10.0.0.1" },
    { stack: "mid", vmid: 2, ip: "10.0.0.2" },
  ];
  const hues = stackHues(nodes);
  assert.equal(hues.size, 3);
  // Sorted order, not input order: alpha first (0°), zeta last.
  assert.equal(hues.get("alpha"), 0);
  assert.equal(hues.get("mid"), 120);
  assert.equal(hues.get("zeta"), 240);
  // Distinct: no two stacks share a hue.
  assert.equal(new Set(hues.values()).size, 3);
});

test("a stack with several edges (several node mentions) still gets exactly one hue", () => {
  // stackHues is handed the node list, which already has one entry per
  // stack; this pins that it de-duplicates rather than assuming that.
  const nodes = [
    { stack: "a", vmid: 1, ip: "10.0.0.1" },
    { stack: "a", vmid: 1, ip: "10.0.0.1" },
    { stack: "b", vmid: 2, ip: "10.0.0.2" },
  ];
  const hues = stackHues(nodes);
  assert.equal(hues.size, 2);
});

test("ten stacks still get ten distinct, evenly spaced hues", () => {
  const nodes = Array.from({ length: 10 }, (_, i) => ({
    stack: `s${String(i).padStart(2, "0")}`,
    vmid: 100 + i,
    ip: `10.0.0.${i}`,
  }));
  const hues = stackHues(nodes);
  assert.equal(new Set(hues.values()).size, 10);
  const sorted = [...hues.values()].sort((a, b) => a - b);
  for (let i = 1; i < sorted.length; i++) {
    assert.equal(
      sorted[i] - sorted[i - 1],
      36,
      "evenly spaced around the wheel",
    );
  }
});
