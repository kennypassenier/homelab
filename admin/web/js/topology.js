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
//
// fix-207 (Kenny, Dutch): "gebruik voor elke stack een andere kleur, dan
// kan ik duidelijk zien wie welke verbinding heeft met wie. Zet het bij in
// de legende en als ik hover over de naam of het bolletje van een stack,
// dan moeten enkel die zijn lijnen getoond worden, de anderen vallen dan
// even weg." — every stack gets its own hue (evenly spaced around the
// wheel, in the same sorted order the layout already uses, so the colour
// a stack gets never depends on which other stacks happen to exist this
// time); every edge is tinted by the stack it leaves FROM; hovering or
// focusing a node's dot, its label, or its legend entry isolates that
// stack's edges, and leaving restores the full graph.

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
 * fix-207: one hue per stack, evenly spaced around the wheel in sorted
 * order — the same order the layout already uses — so the colour a stack
 * gets is deterministic and never collides with a neighbour's.
 * @param {{stack: string}[]} nodes
 * @returns {Map<string, number>}
 */
export function stackHues(nodes) {
  const sorted = [...new Set(nodes.map((n) => n.stack))].sort((a, b) =>
    a.localeCompare(b),
  );
  const hues = new Map();
  sorted.forEach((stack, i) => {
    hues.set(stack, Math.round((360 * i) / Math.max(sorted.length, 1)));
  });
  return hues;
}

/**
 * @typedef {{stack: string, vmid: number, ip: string,
 *   live_enforced?: boolean, live_matches_repo?: boolean}} TopoNode
 * @typedef {{from: string, to: string, kind: "declared"|"open", detail: string[]}} TopoEdge
 * @typedef {{nodes: TopoNode[], edges: TopoEdge[]}} Topology
 */

/**
 * Dim every edge and node that does not touch `stack` (`null` restores
 * everything). Walks the live DOM rather than captured arrays, so it
 * works the same from a node, a label or a legend entry.
 * @param {SVGElement} root
 * @param {string | null} stack
 */
function isolate(root, stack) {
  for (const e of root.querySelectorAll(".topology__edge")) {
    const match =
      stack == null ||
      e.getAttribute("data-from") === stack ||
      e.getAttribute("data-to") === stack;
    e.classList.toggle("topology__edge--dim", !match);
  }
  for (const n of root.querySelectorAll(".topology__node")) {
    const match = stack == null || n.getAttribute("data-stack") === stack;
    n.classList.toggle("topology__node--dim", !match);
  }
}

/**
 * Wires hover and keyboard focus on one element so it isolates `stack` in
 * `root` while active. Shared by a node and its legend entry, so the two
 * behave identically.
 * @param {Element} el
 * @param {SVGElement} root
 * @param {string} stack
 */
function wireIsolate(el, root, stack) {
  el.addEventListener("mouseenter", () => isolate(root, stack));
  el.addEventListener("mouseleave", () => isolate(root, null));
  el.addEventListener("focus", () => isolate(root, stack));
  el.addEventListener("blur", () => isolate(root, null));
}

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
  const hues = stackHues(topo.nodes);
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
      "data-from": e.from,
      "data-to": e.to,
      style: `--stack-hue: ${hues.get(e.from) ?? 0}`,
    });
    line.append(
      svg("title", {}, `${e.from} → ${e.to}: ${e.detail.join(", ") || e.kind}`),
    );
    root.append(line);
  }

  for (const n of topo.nodes) {
    const p = pos.get(n.stack);
    if (!p) continue;
    const hue = hues.get(n.stack) ?? 0;
    const g = svg("g", {
      class: "topology__node",
      "data-stack": n.stack,
      tabindex: "0",
      role: "button",
      style: `--stack-hue: ${hue}`,
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
    // fix-207: a ring around the dot for what the host actually enforces,
    // independent of `n.live_enforced === undefined` (no live answer —
    // nothing drawn, the dot alone still shows the repository's shape).
    if (n.live_enforced === true) {
      g.append(
        svg("circle", {
          cx: String(p.x),
          cy: String(p.y),
          r: String(NODE_R + 4),
          class:
            n.live_matches_repo === false
              ? "topology__fw-ring topology__fw-ring--mismatch"
              : "topology__fw-ring topology__fw-ring--enforced",
        }),
      );
    } else if (n.live_enforced === false && n.live_matches_repo === false) {
      g.append(
        svg("circle", {
          cx: String(p.x),
          cy: String(p.y),
          r: String(NODE_R + 4),
          class: "topology__fw-ring topology__fw-ring--mismatch",
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
    const live =
      n.live_enforced == null
        ? ""
        : n.live_enforced
          ? n.live_matches_repo === false
            ? " · the host enforces this firewall now; the repository disagrees"
            : " · the host enforces this firewall now"
          : n.live_matches_repo === false
            ? " · the host is NOT enforcing this firewall, though the repository disagrees"
            : "";
    g.append(svg("title", {}, `${n.stack} · vmid ${n.vmid} · ${n.ip}${live}`));
    wireIsolate(g, root, n.stack);
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
 * The legend's per-stack swatches, each wired to isolate its stack on the
 * given svg (fix-207): one grid entry per stack, in the same sorted order
 * (and the same hues) the graph itself uses.
 * @param {Topology} topo
 * @param {SVGElement} svgRoot
 */
function stackLegend(topo, svgRoot) {
  const hues = stackHues(topo.nodes);
  const sorted = [...topo.nodes].sort((a, b) => a.stack.localeCompare(b.stack));
  return h(
    "ul",
    { class: "topology__stack-legend", "aria-label": "Stacks" },
    ...sorted.map((n) => {
      const btn = h(
        "button",
        {
          type: "button",
          class: "topology__stack-key",
          style: `--stack-hue: ${hues.get(n.stack) ?? 0}`,
        },
        h("span", { class: "topology__stack-swatch", "aria-hidden": "true" }),
        n.stack,
      );
      wireIsolate(btn, svgRoot, n.stack);
      return h("li", null, btn);
    }),
  );
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
  const svgEl = topologyEl(topo, opts);
  return h(
    "figure",
    { class: "topology" },
    h("figcaption", null, opts.caption ?? "Topology"),
    svgEl,
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
      ...(topo.nodes.some((n) => n.live_enforced != null)
        ? [
            h(
              "li",
              { class: "topology__key topology__key--fw-enforced" },
              "firewall enforced on the host now",
            ),
            h(
              "li",
              { class: "topology__key topology__key--fw-mismatch" },
              "host and repository disagree",
            ),
          ]
        : []),
    ),
    stackLegend(topo, svgEl),
  );
}
