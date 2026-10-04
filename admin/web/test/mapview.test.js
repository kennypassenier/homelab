// redesign-371-map: the Map's pure half (js/mapview.js) — the layout of
// the one topology, its KPI tiles, firewall words, capacity rows and the
// stale images grouped per image.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  bends,
  capacityRows,
  firewallOf,
  hubOf,
  layout,
  mapFigures,
  meterTone,
  neighbours,
  radius,
  shortDate,
  sortCapacity,
  staleGroups,
} from "../js/mapview.js";

/** @type {import("../js/topology.js").Topology} */
const topo = {
  nodes: [
    {
      stack: "admin",
      vmid: 120,
      ip: "10.0.0.20",
      kind: "native",
      live_enforced: true,
      live_matches_repo: true,
    },
    {
      stack: "alpha",
      vmid: 352,
      ip: "10.0.0.92",
      kind: "docker",
      live_enforced: true,
      live_matches_repo: false,
    },
    {
      stack: "beta",
      vmid: 351,
      ip: "10.0.0.91",
      kind: "docker",
      live_enforced: false,
      live_matches_repo: true,
    },
    {
      stack: "edge",
      vmid: 104,
      ip: "10.0.0.4",
      kind: "gateway",
      live_enforced: true,
      live_matches_repo: true,
    },
    { stack: "10.0.0.2", vmid: 0, ip: "10.0.0.2", kind: "external" },
  ],
  edges: [
    { from: "alpha", to: "edge", kind: "declared", detail: ["tcp 3100"] },
    { from: "edge", to: "admin", kind: "route", detail: ["tcp 8090"] },
    { from: "alpha", to: "beta", kind: "open", detail: ["everything"] },
    {
      from: "alpha",
      to: "beta",
      kind: "named",
      detail: ["alpha/lxc-compose.yml"],
    },
    {
      from: "edge",
      to: "10.0.0.2",
      kind: "named",
      detail: ["edge/lxc-compose.yml"],
    },
  ],
};

test("redesign-371-map: the hub sits in the middle, the rest on a ring, the same picture every time", () => {
  assert.equal(hubOf(topo), "edge");
  const pos = layout(topo, 1000, 600);
  const [cx, cy] = /** @type {[number, number]} */ (pos.get("edge"));
  assert.equal(cx, 500);
  assert.equal(cy, 290);
  assert.equal(pos.size, 5);
  // Stacks first in name order, the outside address last: admin at the top.
  const [ax, ay] = /** @type {[number, number]} */ (pos.get("admin"));
  assert.ok(Math.abs(ax - 500) < 0.001 && ay < cy);
  assert.deepEqual(layout(topo, 1000, 600), pos);
  // Without a gateway the busiest node is the hub.
  assert.equal(
    hubOf({
      nodes: topo.nodes.filter((n) => n.kind !== "gateway"),
      edges: topo.edges,
    }),
    "alpha",
  );
});

test("redesign-371-map: two connections between the same pair fan out, a named address always bends", () => {
  const b = bends(topo.edges);
  assert.equal(b[0], 0);
  assert.equal(b[2], 0);
  assert.notEqual(b[3], 0);
  assert.notEqual(b[4], 0);
});

test("redesign-371-map: the tiles count stacks, connections, firewalls and stale images", () => {
  const f = mapFigures(topo, {
    images: [
      {
        key: null,
        where_: "admin/agent",
        pinned: "v0.1.0",
        latest: "v0.2.0",
        upstream: "github.com/x/agent",
      },
      {
        key: "api/api",
        where_: "beta/api",
        pinned: "v2.3.0",
        latest: "v3.0.0",
        upstream: "github.com/x/demo-api",
      },
    ],
  });
  assert.equal(f.stacks, 4);
  assert.equal(f.outside, 1);
  assert.equal(f.connections, 3);
  assert.equal(f.enforced, 2);
  assert.equal(f.open, 1);
  assert.equal(f.firewallOn, 3);
  assert.deepEqual(f.firewallNone, ["beta"]);
  assert.deepEqual(f.firewallDiffers, ["alpha"]);
  assert.equal(f.stale, 2);
  assert.equal(f.major, 1);
  assert.equal(f.updatable, 1);
  assert.equal(mapFigures(topo, null).stale, null);
});

test("redesign-371-map: a node's firewall says what the host enforces, the repository beside it", () => {
  assert.deepEqual(firewallOf(topo.nodes[0]), { text: "in force", tone: "ok" });
  assert.deepEqual(firewallOf(topo.nodes[1]), {
    text: "in force (repository differs)",
    tone: "warn",
  });
  assert.deepEqual(firewallOf(topo.nodes[2]), { text: "none", tone: "bad" });
  assert.deepEqual(firewallOf(topo.nodes[4]), { text: "not read", tone: "" });
  const n = neighbours(topo, "alpha");
  assert.deepEqual(n.dependsOn, ["edge", "beta"]);
  assert.deepEqual(neighbours(topo, "beta").dependedOnBy, ["alpha"]);
  assert.equal(n.edges.length, 3);
});

test("redesign-371-map: a pin in the homelab binary is one row naming every stack; a stack file's pin keeps its own row", () => {
  const g = staleGroups([
    {
      key: null,
      where_: "admin/demo-agent",
      pinned: "v0.1.0",
      latest: "v0.2.0",
      upstream: "github.com/example/demo-agent",
      released: "2026-09-28",
    },
    {
      key: "web/web",
      where_: "beta/demo-web",
      pinned: "1.4.2",
      latest: "1.5.0",
      upstream: "github.com/example/demo-web",
    },
    {
      key: null,
      where_: "kpsite/demo-agent",
      pinned: "v0.1.0",
      latest: "v0.2.0",
      upstream: "github.com/example/demo-agent",
    },
  ]);
  assert.equal(g.length, 2);
  // What this page can update comes first (review finding 13).
  assert.equal(g[1].image, "demo-agent");
  assert.deepEqual(g[1].stacks, ["admin", "kpsite"]);
  assert.equal(g[0].key, "web/web");
  assert.equal(g[0].container, "demo-web");
  assert.equal(g[0].major, false);
  assert.match(shortDate("2026-09-28"), /^Mon 28 Sep( 2026)?$/);
});

test("redesign-371-map: capacity is one row per stack, busiest first, amber from 75% and red from 90%", () => {
  const rows = capacityRows([
    {
      panel: { title: "CPU used" },
      series: [
        { label: "a", points: [[1, 13]] },
        { label: "b", points: [[1, 6]] },
      ],
    },
    {
      panel: { title: "Memory used" },
      series: [{ label: "b", points: [[1, 78]] }],
    },
    {
      panel: { title: "Disk used (root filesystem)" },
      series: [{ label: "", points: [[1, 5]] }],
    },
  ]);
  assert.deepEqual(rows, [
    { stack: "a", cpu: 13, mem: null, disk: null },
    { stack: "b", cpu: 6, mem: 78, disk: null },
  ]);
  assert.deepEqual(
    sortCapacity(rows, "mem").map((r) => r.stack),
    ["b", "a"],
  );
  assert.equal(meterTone(74), "");
  assert.equal(meterTone(75), "warn");
  assert.equal(meterTone(91), "bad");
  assert.equal(radius(topo.nodes[0], null), 14);
  assert.equal(radius(topo.nodes[4], null), 9);
  const t = new Map([
    ["admin", 100],
    ["alpha", 25],
  ]);
  assert.equal(radius(topo.nodes[0], t), 24);
  assert.equal(radius(topo.nodes[1], t), 17);
});
