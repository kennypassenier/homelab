// feat-overview-7 (topology: which container talks to which) and
// feat-firewall-3 (measured traffic drawn on the topology): one inline SVG
// renderer, shared by the Fleet view page (plain) and the Firewall page
// (with the traffic overlay), so the two can never draw the graph two
// different ways.
//
// No chart library (tech-charts, the same choice charts.js made): a simple
// deterministic layout — nodes on a circle, in sorted stack order, so the
// same fleet always draws the same picture and a diff between two days is
// just which edges moved, not which point moved where.

import { h } from "./dom.js";

const NS = "http://www.w3.org/2000/svg";

/**
 * @param {string} tag
 * @param {Record<string, string>} attrs
 * @param {(Node | string)[]} kids
 */
function svg(tag, attrs, ...kids) {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  for (const k of kids) el.append(k);
  return el;
}

const SIZE = 420;
const CENTER = SIZE / 2;
const RADIUS = SIZE / 2 - 56;
const NODE_R = 9;

/**
 * Nodes placed on a circle, sorted by stack name (deterministic: the same
 * fleet always lays out the same way).
 * @param {{stack: string}[]} nodes
 * @returns {Map<string, {x: number, y: number}>}
 */
function layout(nodes) {
  const sorted = [...nodes].sort((a, b) => a.stack.localeCompare(b.stack));
  const pos = new Map();
  sorted.forEach((n, i) => {
    const a = (2 * Math.PI * i) / Math.max(sorted.length, 1) - Math.PI / 2;
    pos.set(n.stack, {
      x: CENTER + RADIUS * Math.cos(a),
      y: CENTER + RADIUS * Math.sin(a),
    });
  });
  return pos;
}

/**
 * @typedef {{stack: string, vmid: number, ip: string}} TopoNode
 * @typedef {{from: string, to: string, kind: "declared"|"open", detail: string[]}} TopoEdge
 * @typedef {{nodes: TopoNode[], edges: TopoEdge[]}} Topology
 */

/**
 * @param {Topology} topo
 * @param {{traffic?: Map<string, number>, onSelect?: (stack: string) => void}} [opts]
 *   `traffic` is bytes/s per stack (received + transmitted), when the
 *   caller has it (feat-firewall-3) — drawn as each node's ring thickness,
 *   since the fleet has no per-edge flow metric to put on the edges
 *   themselves (see core/src/charts.rs's `fleet_traffic_panels`).
 */
export function topologyEl(topo, opts = {}) {
  const pos = layout(topo.nodes);
  const root = svg("svg", {
    viewBox: `0 0 ${SIZE} ${SIZE}`,
    class: "topology__svg",
    role: "img",
    "aria-label": "Which container talks to which",
  });
  const maxTraffic = opts.traffic
    ? Math.max(1, ...[...opts.traffic.values()])
    : 0;

  for (const e of topo.edges) {
    const a = pos.get(e.from);
    const b = pos.get(e.to);
    if (!a || !b) continue;
    const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
    // A slight curve (quadratic, bowed toward the centre) so the two
    // directions between the same pair of stacks never draw on top of
    // each other.
    const bow = {
      x: mid.x + (CENTER - mid.x) * 0.15,
      y: mid.y + (CENTER - mid.y) * 0.15,
    };
    const line = svg("path", {
      d: `M ${a.x} ${a.y} Q ${bow.x} ${bow.y} ${b.x} ${b.y}`,
      class: `topology__edge topology__edge--${e.kind}`,
    });
    line.append(
      svg("title", {}, `${e.from} → ${e.to}: ${e.detail.join(", ") || e.kind}`),
    );
    root.append(line);
  }

  for (const n of topo.nodes) {
    const p = pos.get(n.stack);
    if (!p) continue;
    const g = svg("g", {
      class: "topology__node",
      tabindex: "0",
      role: "button",
      "aria-label": `${n.stack} (vmid ${n.vmid}, ${n.ip})`,
    });
    const traffic = opts.traffic?.get(n.stack) ?? 0;
    const ringR = NODE_R + (opts.traffic ? 6 * (traffic / maxTraffic) : 0);
    if (opts.traffic && traffic > 0) {
      g.append(
        svg("circle", {
          cx: String(p.x),
          cy: String(p.y),
          r: String(ringR),
          class: "topology__traffic-ring",
        }),
      );
    }
    g.append(
      svg("circle", {
        cx: String(p.x),
        cy: String(p.y),
        r: String(NODE_R),
        class: "topology__dot",
      }),
    );
    const below = p.y > CENTER;
    g.append(
      svg(
        "text",
        {
          x: String(p.x),
          y: String(p.y + (below ? NODE_R + 14 : -NODE_R - 6)),
          class: "topology__label",
          "text-anchor": "middle",
        },
        n.stack,
      ),
    );
    g.append(svg("title", {}, `${n.stack} · vmid ${n.vmid} · ${n.ip}`));
    if (opts.onSelect) {
      g.addEventListener("click", () => opts.onSelect?.(n.stack));
      g.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          opts.onSelect?.(n.stack);
        }
      });
    }
    root.append(g);
  }
  return root;
}

/**
 * The figure wrapper (caption + legend) both pages use around
 * {@link topologyEl}.
 * @param {Topology} topo
 * @param {{traffic?: Map<string, number>, onSelect?: (stack: string) => void, caption?: string}} [opts]
 */
export function topologyFigure(topo, opts = {}) {
  if (!topo.nodes.length) {
    return h(
      "p",
      { class: "chart__empty" },
      "No stacks with a working copy to draw yet.",
    );
  }
  return h(
    "figure",
    { class: "topology" },
    h("figcaption", null, opts.caption ?? "Topology"),
    topologyEl(topo, opts),
    h(
      "ul",
      { class: "topology__legend" },
      h(
        "li",
        { class: "topology__key topology__key--declared" },
        "declared flow",
      ),
      h(
        "li",
        { class: "topology__key topology__key--open" },
        "no firewall on the target (open)",
      ),
      ...(opts.traffic
        ? [
            h(
              "li",
              { class: "topology__key topology__key--traffic" },
              "measured traffic (ring)",
            ),
          ]
        : []),
    ),
  );
}
