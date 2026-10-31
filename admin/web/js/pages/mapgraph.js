// The Map's ONE topology (fix-215, invariant 24) on kp-themes' network graph
// (11.2.0, gap 102, graph-1020: Kenny chose "Volledig" on 2026-10-20). The
// drawing itself is the package's: the hub in the middle, every other stack
// on a ring round it, the addresses outside the fleet dashed, one fixed
// colour per stack (invariant 16), the connection kinds told apart by line
// style, an arrowhead where a connection goes to, names whole under their
// nodes, Show all and the hint inside the drawing, wheel/pinch zoom and
// drag pan, and a details panel for a picked stack. This module only turns
// the Map's topology into the graph's data, names the graph's controls for
// Live view and carries the Map's selection in and out.
//
//   hover / focus a node                       isolate its connections
//   click / Enter                              turn the node on or off in
//                                              the selection (several may
//                                              be on, no modifier keys)
//   Show all, Esc                              clear the selection and
//                                              every hidden kind
//
// A hover only toggles classes, never rebuilds a node (invariant 56: a
// click right after the pointer moved must land on the node under it);
// that is the graph's own rule. Only a resize, the traffic switch and the
// data itself draw it again.

import { h } from "../dom.js";
import {
  GRAPH_CHANGE_EVENT,
  attachGraphs,
  graphHideKind,
  graphSelect,
  setGraphData,
  setGraphState,
  registerShortcuts,
} from "/static/kp/dist/kp-themes.min.js";
import { GROUP_MAP, unlessDialog } from "../shortcuts.js";
import { declare, drivable } from "../drivable.js";
import { EDGE_KINDS, hubOf, hues, isStack } from "../mapview.js";

const NODE = declare({
  id: "map-node",
  page: "fleetview",
  opens: "view",
  row: "<stack>",
  what: "turn a stack (or an outside address) on or off in the Map's selection, showing its connections and details",
});
const SHOW_ALL = declare({
  id: "map-show-all",
  page: "fleetview",
  opens: "view",
  what: "clear the Map's selection and show every connection kind again",
  shows: "while a node is selected",
  reach: [{ do: "click", control: "map-node", row: "*" }],
});
const KIND = declare({
  id: "map-edge-kind",
  page: "fleetview",
  opens: "view",
  row: "declared|planned|open|named|route",
  what: "show or hide one kind of connection on the topology",
});

/**
 * The Map's connection kinds as the graph draws them: the line style and
 * colour each kind has always had on the Map.
 * @type {Record<string, {style: "solid" | "dash" | "dot" | "long-dash", colour: string}>}
 */
const LOOK = {
  declared: { style: "solid", colour: "var(--chart-3)" },
  planned: { style: "dash", colour: "var(--muted-foreground)" },
  open: { style: "long-dash", colour: "var(--mk-warn)" },
  named: { style: "dot", colour: "var(--chart-2)" },
  route: { style: "solid", colour: "var(--chart-1)" },
};

/**
 * @typedef {import("../topology.js").Topology} Topology
 * @typedef {import("/static/kp/dist/kp-themes.min.js").GraphData} GraphData
 */

/**
 * The topology as the graph's data. A node's weight is its share of the
 * busiest stack's measured traffic (the graph sizes it 10 to 24 px by the
 * square root, 14 px without), an address outside the fleet is the graph's
 * own smaller dashed node, a stack whose live firewall differs from the
 * repository carries the graph's mismatch ring.
 * @param {Topology} topo
 * @param {Map<string, number> | null} traffic
 * @returns {GraphData}
 */
export function graphData(topo, traffic) {
  const hue = hues(topo);
  const max = traffic ? Math.max(1, ...traffic.values()) : 1;
  return {
    ...(hubOf(topo) ? { hub: /** @type {string} */ (hubOf(topo)) } : {}),
    nodes: topo.nodes.map((n) => ({
      id: n.stack,
      label: n.stack,
      description: isStack(n)
        ? `CT ${n.vmid}, ${n.ip}`
        : "an address outside the fleet",
      ...(isStack(n)
        ? {
            hue: hue.get(n.stack) ?? 0,
            ...(traffic ? { weight: (traffic.get(n.stack) ?? 0) / max } : {}),
          }
        : { external: true }),
      ...(n.live_enforced != null && n.live_matches_repo === false
        ? { flag: /** @type {const} */ ("mismatch") }
        : {}),
    })),
    edges: topo.edges.map((e) => ({
      from: e.from,
      to: e.to,
      kind: e.kind,
      detail: e.detail.join(", ") || e.kind,
    })),
    kinds: EDGE_KINDS.map((k) => ({
      kind: k.kind,
      label: k.label,
      hint: k.hint,
      ...(LOOK[k.kind] ?? LOOK.declared),
    })),
  };
}

/**
 * The graph, one per Map page. It exists from the first frame (loading
 * with the theme's own picture), gets the topology with `show`, and keeps
 * the Map's selection, hover and hidden kinds across a redraw.
 * @param {{traffic: () => Map<string, number> | null,
 *   detail: (id: string) => Node | null,
 *   onChange: (selected: string[], hover: string | null) => void}} opts
 */
export function mapGraph(opts) {
  /** @type {Topology | null} */
  let topo = null;
  /** @type {string[]} */
  let selected = [];
  /** @type {string[]} */
  let hiddenKinds = [];
  /** @type {(() => void) | null} */
  let detach = null;

  // The graph's options are its attributes (kp-themes 11.2.0): names under
  // the nodes, Show all and the hint inside, one fixed colour per stack,
  // arrowheads, zoom and pan, the details panel, and the Map's own radii
  // (the ring a share of the picture, the node in px).
  const el = h("figure", {
    class: "kp-graph",
    id: "map-graph",
    "data-kp-graph": true,
    "data-kp-key": "map-graph",
    "data-kp-labels": "below",
    "data-kp-chrome": "inside",
    "data-kp-colour": "fixed",
    "data-kp-direction": "arrows",
    "data-kp-zoom": true,
    "data-kp-detail": true,
    "data-kp-ring-radius": "0.42",
    "data-kp-node-radius": "14",
    "aria-label":
      "Topology: Tab to a stack, Enter to add it to the selection or take it out",
  });
  let hover = /** @type {string | null} */ (null);
  el.addEventListener(GRAPH_CHANGE_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    selected = d.selected;
    hiddenKinds = d.hiddenKinds;
    hover = d.hover;
    opts.onChange(selected, hover);
  });

  /** Name the graph's controls for Live view; it calls this on every draw. */
  const decorate = (
    /** @type {Element} */ part,
    /** @type {import("/static/kp/dist/kp-themes.min.js").GraphDecorateInfo} */ info,
  ) => {
    const p = /** @type {HTMLElement} */ (part);
    if (info.kind === "node" && info.value) drivable(p, NODE, info.value);
    else if (info.kind === "kind" && info.value) drivable(p, KIND, info.value);
    else if (info.kind === "show-all") drivable(p, SHOW_ALL);
  };
  const attach = () => {
    detach ??= attachGraphs(el, {
      decorate,
      detail: ({ id }) => opts.detail(id),
    });
  };

  const clear = () => {
    for (const k of hiddenKinds) graphHideKind(el, k, false);
    graphSelect(el, []);
  };
  const stopKeys = registerShortcuts("page:map", [
    {
      keys: "Escape",
      what: "Unpin the stacks and show every kind again",
      group: GROUP_MAP,
      when: () => selected.length > 0 || hiddenKinds.length > 0,
      reason: "Nothing is pinned or hidden",
      run: unlessDialog(() => {
        if (selected.length === 0 && hiddenKinds.length === 0) return false;
        clear();
        return undefined;
      }),
    },
  ]);

  const paint = () => {
    if (topo) setGraphData(el, graphData(topo, opts.traffic()));
  };
  return {
    el,
    /** The theme's own loading picture, at the final height. @param {string} words */
    loading: (words) => {
      attach();
      setGraphState(el, "loading", words);
    },
    /**
     * The theme's failed plate over the picture, with its own Retry
     * (`kp-graph-retry` on the figure).
     * @param {string} title @param {string} text
     */
    failed: (title, text) => {
      attach();
      setGraphState(el, "error", title, { text });
    },
    /**
     * Draw the topology; `pick` is the selection to start with (the
     * address's `select`), used only the first time.
     * @param {Topology} t @param {string[]} [pick]
     */
    show: (t, pick = []) => {
      const first = topo === null;
      topo = t;
      attach();
      paint();
      if (first && pick.length) graphSelect(el, pick);
    },
    /** The view came back on screen or the data moved: draw it again. */
    draw: paint,
    /** @param {string} kind @param {boolean} hide */
    hideKind: (kind, hide) => graphHideKind(el, kind, hide),
    selected: () => [...selected],
    /** The traffic switch moved: the nodes are sized again. */
    setTraffic: paint,
    stop: () => {
      detach?.();
      detach = null;
      stopKeys();
    },
  };
}
