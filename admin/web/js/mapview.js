// redesign-371-map (release 3.71.0; Kenny approved the demo 2026-10-03:
// ~/.local/share/homelab/redesign-3.71/fleetview.html + fleetview.js, the
// Fleet view renamed Map): the Map page's pure half — where each node of
// the ONE topology sits, what the side panel and the KPI tiles say, the
// capacity rows and the stale images grouped per image — so it is tested
// without a browser. js/pages/fleetview.js draws what these return.

import { stackHues } from "./topology.js";
import { majorJump } from "./staleimages.js";

/**
 * @typedef {import("./topology.js").TopoNode} TopoNode
 * @typedef {import("./topology.js").TopoEdge} TopoEdge
 * @typedef {import("./topology.js").Topology} Topology
 */

/**
 * The kinds of connection, in the legend's order, with the demo's words.
 * @type {{kind: TopoEdge["kind"], label: string, hint: string}[]}
 */
export const EDGE_KINDS = [
  {
    kind: "declared",
    label: "declared, firewall enforced",
    hint: "A connection a stack's firewall rules allow, and the host enforces",
  },
  {
    kind: "planned",
    label: "declared, firewall not on yet",
    hint: "A connection a stack declares, on a stack whose firewall is not switched on",
  },
  {
    kind: "open",
    label: "target has no firewall (open)",
    hint: "Anything may connect: the target stack has no firewall",
  },
  {
    kind: "named",
    label: "address named in a stack file",
    hint: "An address a stack's own files name, outside any firewall rule",
  },
  {
    kind: "route",
    label: "gateway route to a backend",
    hint: "The gateway forwards a hostname to this stack",
  },
];

/**
 * The hub: the node a stack's gateway route names (kind `gateway`), else
 * the one with the most connections.
 * @param {Topology} topo
 * @returns {string | null}
 */
export function hubOf(topo) {
  const gw = topo.nodes.find((n) => n.kind === "gateway");
  if (gw) return gw.stack;
  /** @type {Map<string, number>} */
  const deg = new Map();
  for (const e of topo.edges) {
    deg.set(e.from, (deg.get(e.from) ?? 0) + 1);
    deg.set(e.to, (deg.get(e.to) ?? 0) + 1);
  }
  let best = null;
  let n = -1;
  for (const node of topo.nodes) {
    const d = deg.get(node.stack) ?? 0;
    if (d > n) {
      n = d;
      best = node.stack;
    }
  }
  return best;
}

/**
 * Where every node sits in a W×H box (the demo's layout): the hub in the
 * middle, the others on an ellipse around it in a stable order — the
 * fleet's stacks sorted by name, then the addresses outside it — so the
 * same fleet always draws the same picture.
 * @param {Topology} topo
 * @param {number} W
 * @param {number} H
 * @returns {Map<string, [number, number]>}
 */
export function layout(topo, W, H) {
  /** @type {Map<string, [number, number]>} */
  const pos = new Map();
  const hub = hubOf(topo);
  const cx = W / 2;
  const cy = H / 2 - 10;
  const R = Math.min(W, H) * 0.36;
  const sx = W > 600 ? 1.35 : 1;
  if (hub) pos.set(hub, [cx, cy]);
  const ring = topo.nodes
    .filter((n) => n.stack !== hub)
    .sort(
      (a, b) =>
        Number(a.kind === "external") - Number(b.kind === "external") ||
        a.stack.localeCompare(b.stack),
    );
  ring.forEach((n, i) => {
    const a = -Math.PI / 2 + (i / Math.max(1, ring.length)) * Math.PI * 2;
    pos.set(n.stack, [cx + Math.cos(a) * R * sx, cy + Math.sin(a) * R]);
  });
  return pos;
}

/**
 * Each edge's bend: edges between the same two nodes fan out (0, +26,
 * −26, +52, …) so two kinds of connection never draw on top of each
 * other; a named address always bends.
 * @param {TopoEdge[]} edges
 * @returns {number[]}
 */
export function bends(edges) {
  /** @type {Map<string, number>} */
  const seen = new Map();
  return edges.map((e) => {
    const key = [e.from, e.to].sort().join("\u0000");
    const k = seen.get(key) ?? 0;
    seen.set(key, k + 1);
    const step = e.kind === "named" ? k + 1 : k;
    if (step === 0) return 0;
    const mag = Math.ceil(step / 2) * 26;
    // The same bend reads opposite ways for a → b and b → a.
    const dir = (step % 2 ? 1 : -1) * (e.from < e.to ? 1 : -1);
    return mag * dir;
  });
}

/** @param {TopoNode} n */
export const isStack = (n) => n.kind !== "external";

/**
 * A node's firewall in words and tone, from what the host enforces now
 * (invariant 17), the repository's word beside it when they differ.
 * @param {TopoNode} n
 * @returns {{text: string, tone: "ok" | "warn" | "bad" | ""}}
 */
export function firewallOf(n) {
  if (n.live_enforced == null) return { text: "not read", tone: "" };
  if (n.live_enforced)
    return n.live_matches_repo === false
      ? { text: "in force (repository differs)", tone: "warn" }
      : { text: "in force", tone: "ok" };
  return n.live_matches_repo === false
    ? { text: "off (repository says on)", tone: "bad" }
    : { text: "none", tone: "bad" };
}

/**
 * What a node depends on and what depends on it (named addresses are not
 * a dependency), each name once.
 * @param {Topology} topo
 * @param {string} id
 */
export function neighbours(topo, id) {
  const real = topo.edges.filter((e) => e.kind !== "named");
  const uniq = (/** @type {string[]} */ a) => [...new Set(a)];
  return {
    dependsOn: uniq(real.filter((e) => e.from === id).map((e) => e.to)),
    dependedOnBy: uniq(real.filter((e) => e.to === id).map((e) => e.from)),
    edges: topo.edges.filter((e) => e.from === id || e.to === id),
  };
}

/**
 * The four KPI tiles above the topology.
 * @param {Topology} topo
 * @param {{images?: any[]} | null} stale
 */
export function mapFigures(topo, stale) {
  const stacks = topo.nodes.filter(isStack);
  const outside = topo.nodes.length - stacks.length;
  const real = topo.edges.filter((e) => e.kind !== "named");
  const enforced = real.filter(
    (e) => e.kind === "declared" || e.kind === "route",
  ).length;
  const open = real.filter((e) => e.kind === "open").length;
  const planned = real.filter((e) => e.kind === "planned").length;
  const on = stacks.filter((n) => n.live_enforced === true).length;
  const none = stacks
    .filter((n) => n.live_enforced === false)
    .map((n) => n.stack);
  const differs = stacks
    .filter((n) => n.live_enforced === true && n.live_matches_repo === false)
    .map((n) => n.stack);
  const groups = stale ? staleGroups(stale.images ?? []) : null;
  return {
    stacks: stacks.length,
    outside,
    connections: real.length,
    enforced,
    open,
    planned,
    firewallOn: on,
    firewallNone: none,
    firewallDiffers: differs,
    stale: groups?.length ?? null,
    major: groups?.filter((g) => g.major).length ?? 0,
    updatable: groups?.filter((g) => g.key).length ?? 0,
  };
}

/**
 * @typedef {{image: string, stacks: string[], where: string[],
 *   key: string | null, pinned: string, latest: string, upstream: string,
 *   released: string | null, major: boolean, container: string}}
 *   StaleGroup one row of Stale images
 */

/**
 * `/data/stale-images` rows as the demo lists them: one row per image and
 * version step. A pin no stack file holds (`key` null: it lives in the
 * homelab binary) is one row naming every stack that runs it; a pin a stack
 * file holds stays one row per stack, since each has its own Update.
 * @param {any[]} images
 * @returns {StaleGroup[]}
 */
export function staleGroups(images) {
  /** @type {Map<string, StaleGroup>} */
  const out = new Map();
  for (const x of images) {
    const [stack, container = ""] = String(x.where_).split("/");
    const image =
      String(x.upstream ?? "")
        .split("/")
        .filter(Boolean)
        .pop() || container;
    const id = x.key
      ? `${x.where_}|${x.key}`
      : `${image}|${x.pinned}|${x.latest}`;
    const g = out.get(id);
    if (g) {
      if (!g.stacks.includes(stack)) g.stacks.push(stack);
      g.where.push(x.where_);
      continue;
    }
    out.set(id, {
      image,
      stacks: [stack],
      where: [x.where_],
      key: x.key ?? null,
      pinned: x.pinned,
      latest: x.latest,
      upstream: x.upstream,
      released: x.released ?? null,
      major: majorJump(x.pinned, x.latest),
      container,
    });
  }
  // Review finding 13: the jumps that need reading first, then what this
  // page can update, then the pins a homelab release moves; by name within.
  const rank = (/** @type {StaleGroup} */ g) =>
    (g.major ? 0 : 2) + (g.key ? 0 : 1);
  return [...out.values()].sort(
    (a, b) =>
      rank(a) - rank(b) ||
      a.image.localeCompare(b.image) ||
      a.stacks.join(",").localeCompare(b.stacks.join(",")),
  );
}

/**
 * Disk growth rows in one fixed order, whatever order a read brings them
 * in: the soonest full first (a filesystem that is not growing last), then
 * the fullest, then by name.
 * @template {{subject?: string, scope?: string,
 *   fit: {days_to_full: number | null, pct_now: number}}} R
 * @param {R[]} rows
 * @returns {R[]}
 */
export function sortGrowth(rows) {
  const days = (/** @type {R} */ r) =>
    r.fit.days_to_full == null ? Infinity : r.fit.days_to_full;
  const name = (/** @type {R} */ r) => `${r.scope ?? ""} ${r.subject ?? ""}`;
  return [...rows].sort(
    (a, b) =>
      days(a) - days(b) ||
      b.fit.pct_now - a.fit.pct_now ||
      name(a).localeCompare(name(b)),
  );
}

/**
 * "30 Sep" from a release date ("2026-09-30").
 * @param {string | null} iso
 */
export function shortDate(iso) {
  if (!iso) return "—";
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const mon = "Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ")[
    Number(m[2]) - 1
  ];
  return `${Number(m[3])} ${mon}`;
}

/**
 * @typedef {{stack: string, cpu: number | null, mem: number | null,
 *   disk: number | null}} CapRow
 */

/**
 * `/data/capacity`'s three panels as one row per stack.
 * @param {any[]} panels
 * @returns {CapRow[]}
 */
export function capacityRows(panels) {
  /** @type {Map<string, CapRow>} */
  const rows = new Map();
  for (const p of panels ?? []) {
    const t = String(p.panel?.title ?? "");
    const key = t.startsWith("CPU")
      ? "cpu"
      : t.startsWith("Memory")
        ? "mem"
        : "disk";
    for (const s of p.series ?? []) {
      if (!s.label || !s.points?.length) continue;
      const row = rows.get(s.label) ?? {
        stack: s.label,
        cpu: null,
        mem: null,
        disk: null,
      };
      row[key] = s.points[s.points.length - 1][1];
      rows.set(s.label, row);
    }
  }
  return [...rows.values()];
}

/**
 * Busiest first by the chosen measure; a stack without that reading last.
 * @param {CapRow[]} rows
 * @param {"cpu" | "mem" | "disk"} by
 */
export function sortCapacity(rows, by) {
  return [...rows].sort(
    (a, b) => (b[by] ?? -1) - (a[by] ?? -1) || a.stack.localeCompare(b.stack),
  );
}

/**
 * A meter's tone (the card's foot: amber from 75%, red from 90%).
 * @param {number | null} pct
 */
export const meterTone = (pct) =>
  pct == null ? "" : pct >= 90 ? "bad" : pct >= 75 ? "warn" : "";

/**
 * A node's colour: its stack's evenly spaced hue (topology.js, the same
 * hue on the Backups page), muted for an address outside the fleet.
 * @param {Topology} topo
 */
export function hues(topo) {
  return stackHues(topo.nodes.filter(isStack));
}

/**
 * Measured traffic (bytes/s per stack, received + transmitted) from
 * `/data/fleet-traffic`'s panels.
 * @param {any[]} panels
 * @returns {Map<string, number>}
 */
export function trafficByStack(panels) {
  /** @type {Map<string, number>} */
  const m = new Map();
  for (const p of panels ?? [])
    for (const s of p.series ?? []) {
      if (!s.points?.length) continue;
      m.set(s.label, (m.get(s.label) ?? 0) + s.points[s.points.length - 1][1]);
    }
  return m;
}

/**
 * A node's radius: 14 px, or with measured traffic on, 10 to 24 px by the
 * square root of its share of the busiest stack's traffic.
 * @param {TopoNode} n
 * @param {Map<string, number> | null} traffic
 */
export function radius(n, traffic) {
  if (!isStack(n)) return 9;
  if (!traffic) return 14;
  const max = Math.max(1, ...traffic.values());
  return 10 + 14 * Math.sqrt((traffic.get(n.stack) ?? 0) / max);
}
