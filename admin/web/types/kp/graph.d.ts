/** A hub-and-ring graph: `figure.kp-graph[data-kp-graph]`. */
export declare const GRAPH = "[data-kp-graph]";
/** Fired on the graph, bubbling, when its selection, hover or hidden kinds change: `detail` `{ selected, hover, hiddenKinds }`. */
export declare const GRAPH_CHANGE_EVENT = "kp-graph-change";
/** Fired on the graph, bubbling, when the Retry of its failed state is pressed [state-plates-1019]. */
export declare const GRAPH_RETRY_EVENT = "kp-graph-retry";
/** Fired on the graph, bubbling, when a picked node's details open [graph-1020]: `detail` `{ id, node, slot, host }`; the page fills `slot`. */
export declare const GRAPH_DETAIL_EVENT = "kp-graph-detail";
/** The attributes that are the graph's options; a change to one redraws [graph-1020]. */
export declare const GRAPH_OPTION_ATTRIBUTES: string[];
export type GraphNode = {
  id: string;
  label: string;
  kind?: string;
  hue?: number;
  colour?: string;
  radius?: number;
  weight?: number | null;
  flag?: "mismatch" | null;
  description: string;
  external?: boolean;
};
export type GraphEdge = {
  from: string;
  to: string;
  kind: string;
  detail?: string;
  arrows?: "to" | "from" | "both" | "none";
};
export type GraphKind = {
  kind: string;
  label: string;
  hint: string;
  style: "solid" | "dash" | "dot" | "long-dash";
  colour: string;
};
export type GraphData = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  kinds: GraphKind[];
  hub?: string;
};
export type GraphDecorateInfo = {
  kind:
    | "node"
    | "kind"
    | "show-all"
    | "zoom-in"
    | "zoom-out"
    | "zoom-reset"
    | "detail-close";
  host: HTMLElement;
  key?: string;
  index?: number;
  label?: string;
  value?: string;
};
export type GraphDetailInfo = {
  id: string;
  node: GraphNode;
  slot: HTMLElement;
  host: HTMLElement;
  key?: string;
};
export type GraphOptions = {
  labels: "beside" | "below";
  chrome: "outside" | "inside";
  colour: "theme" | "fixed";
  direction: "none" | "arrows";
  zoom: boolean;
  detail: boolean;
  ring: number | null;
  node: number | null;
};
/**
 * @typedef {{ id: string, label: string, kind?: string, hue?: number, colour?: string, radius?: number, weight?: number | null,
 *   flag?: 'mismatch' | null, description: string, external?: boolean }} GraphNode
 * @typedef {{ from: string, to: string, kind: string, detail?: string, arrows?: 'to' | 'from' | 'both' | 'none' }} GraphEdge
 * @typedef {{ kind: string, label: string, hint: string, style: 'solid' | 'dash' | 'dot' | 'long-dash', colour: string }} GraphKind
 * @typedef {{ nodes: GraphNode[], edges: GraphEdge[], kinds: GraphKind[], hub?: string }} GraphData
 */
/**
 * What `decorate` is told about the control it is handed: the same shape every kp module's `decorate` takes
 * (`DataTableDecorateInfo`). `kind` is `node` (a node of the picture; `value` is its id, `label` its name), `kind` (a
 * toggle in the list of kinds; `value` is the kind), `show-all`, or one of the option controls [graph-1020]: `zoom-in`, `zoom-out`, `zoom-reset` and `detail-close` (`value` is the node whose details they close). `host` is the graph and `key` its `data-kp-key`.
 * @typedef {{ kind: 'node' | 'kind' | 'show-all' | 'zoom-in' | 'zoom-out' | 'zoom-reset' | 'detail-close', host: HTMLElement, key?: string, index?: number, label?: string, value?: string }} GraphDecorateInfo
 */
/**
 * What the page hands the `detail` hook, and what the `kp-graph-detail` event carries: the picked node, the slot to fill
 * (an element inside the details panel) and the graph.
 * @typedef {{ id: string, node: GraphNode, slot: HTMLElement, host: HTMLElement, key?: string }} GraphDetailInfo
 * @typedef {{ labels: 'beside' | 'below', chrome: 'outside' | 'inside', colour: 'theme' | 'fixed', direction: 'none' | 'arrows',
 *   zoom: boolean, detail: boolean, ring: number | null, node: number | null }} GraphOptions
 */
/**
 * The graph's options as its attributes say them; anything missing or unreadable is the approved look.
 * @param {Element} host
 * @returns {GraphOptions}
 */
export declare function graphOptionsOf(host: Element): GraphOptions;
/**
 * Set options on a graph (they are attributes, so markup and script say the same thing); `null` or `false` takes one off.
 * @param {Element} el
 * @param {Partial<{ [K in keyof GraphOptions]: GraphOptions[K] | null | false }>} options
 */
export declare function setGraphOptions(
  el: Element,
  options: Partial<{
    [K in keyof GraphOptions]: GraphOptions[K] | null | false;
  }>,
): void;
/**
 * The hub: the one named, else the node with the most links.
 * @param {GraphData} data
 * @returns {string | null}
 */
export declare function hubOf(data: GraphData): string | null;
/**
 * The ring's order: the nodes by label, then the outside ones; the first at
 * the top, clockwise.
 * @param {GraphData} data
 * @param {string | null} hub
 * @returns {GraphNode[]}
 */
export declare const ringOf: (
  data: GraphData,
  hub: string | null,
) => GraphNode[];
export type GraphLayoutOptions = {
  ring?: number;
  inset?: {
    top?: number;
    bottom?: number;
  };
  phase?: number;
  stretch?: number;
};
export type GraphRing = {
  cx: number;
  cy: number;
  rx: number;
  ry: number;
};
/**
 * Where every node sits in a W×H box: the hub in the middle, the others on an
 * ellipse around it (stretched sideways on a wide box). On a narrow box the
 * ellipse gives up width for height, so the labels beside the ring keep room.
 * `options.ring` sizes the ring (a share of the box, 0.36) and `options.inset`
 * keeps a strip at the top or the bottom clear and `options.phase` turns the ring by a share of the step between two nodes
 * (0, the approved look, unless a page asks) [graph-1020].
 * @param {GraphData} data
 * @param {number} W
 * @param {number} H
 * @param {GraphLayoutOptions} [options]
 * @returns {Map<string, { x: number, y: number, angle: number }>}
 */
export declare const graphLayout: (
  data: GraphData,
  W: number,
  H: number,
  options?: GraphLayoutOptions,
) => Map<
  string,
  {
    x: number;
    y: number;
    angle: number;
  }
>;
/**
 * The layout for names that stand whole under their nodes [graph-1020]: the ring is narrowed until every name lies inside the
 * box, and the box grows taller (up to `maxHeight`) until no name touches another or a node. Nothing is cut: the picture gets the
 * room the names need. `widthOf(node)` is the width of the node's name as drawn.
 *
 * When names so long that even the tallest picture cannot hold them on one line are given a `wrapOf(node, target)`, the same search runs
 * again with every name wider than `target` on several centred lines (`{ width, lines }`, the widest line's width) and the
 * best of the tries wins; the lines come back in `wraps` by node id. Names that fit stay on one line, so a set that cleared before
 * is drawn as it was.
 * @param {GraphData} data
 * @param {number} W
 * @param {number} H
 * @param {(node: GraphNode) => number} widthOf
 * @param {GraphLayoutOptions & { radiusOf?: (node: GraphNode) => number, line?: number, maxHeight?: number, wrapOf?: (node: GraphNode, target: number) => { width: number, lines: string[] } | null }} [options]
 * @returns {{ at: Map<string, { x: number, y: number, angle: number }>, H: number, wraps: Map<string, string[]> }}
 */
export declare function graphRoom(
  data: GraphData,
  W: number,
  H: number,
  widthOf: (node: GraphNode) => number,
  {
    radiusOf: rOf,
    line,
    maxHeight,
    wrapOf,
    ...layout
  }?: GraphLayoutOptions & {
    radiusOf?: (node: GraphNode) => number;
    line?: number;
    maxHeight?: number;
    wrapOf?: (
      node: GraphNode,
      target: number,
    ) => {
      width: number;
      lines: string[];
    } | null;
  },
): {
  at: Map<
    string,
    {
      x: number;
      y: number;
      angle: number;
    }
  >;
  H: number;
  wraps: Map<string, string[]>;
};
/**
 * The arrowhead of a link [graph-1020]: a triangle whose tip stands `clear` px short of the node at the link's end, turned to
 * follow the curve there. The link is the quadratic curve `from`, `bend`, `to`. `at: 'from'` puts it at the other end.
 * @param {{ x: number, y: number }} from
 * @param {{ x: number, y: number }} bend
 * @param {{ x: number, y: number }} to
 * @param {number} clear  how far from the end node's centre the tip stands (its radius and a gap)
 * @param {number} [size]
 * @returns {string} the `d` of the head
 */
export declare function graphArrow(
  from: {
    x: number;
    y: number;
  },
  bend: {
    x: number;
    y: number;
  },
  to: {
    x: number;
    y: number;
  },
  clear: number,
  size?: number,
): string;
/**
 * Each edge's bend: edges between the same two nodes fan out (0, +26, −26,
 * +52 …) so two kinds never draw on top of each other.
 * @param {GraphEdge[]} edges
 * @returns {number[]}
 */
export declare function graphBends(edges: GraphEdge[]): number[];
/**
 * A label kept to `length` letters: the whole name, or its first letters and
 * an ellipsis (`Pump h…`).
 * @param {string} full
 * @param {number} length
 * @returns {string}
 */
export declare const graphLabelText: (full: string, length: number) => string;
export type GraphLabel = {
  full: string;
  length: number;
};
export type GraphBox = {
  x: number;
  y: number;
  width: number;
  height: number;
};
/**
 * @typedef {{ full: string, length: number }} GraphLabel
 * @typedef {{ x: number, y: number, width: number, height: number }} GraphBox
 */
/**
 * Shorten labels until each lies inside the W×H box and no two touch: the
 * longer of two that touch loses a letter, an ellipsis marks the cut, and the
 * full name stays in the node's title and accessible name. `measure(label)`
 * gives the box of the label as `graphLabelText(label.full, label.length)`
 * draws it; each label's `length` is shortened in place.
 * @template {GraphLabel} L
 * @param {L[]} labels
 * @param {number} W
 * @param {number} H
 * @param {(label: L) => GraphBox} measure
 */
export declare function fitGraphLabels<L extends GraphLabel>(
  labels: L[],
  W: number,
  H: number,
  measure: (label: L) => GraphBox,
): void;
export type DrawnLabel = GraphLabel & {
  text: SVGTextElement;
};
export type GraphView = {
  k: number;
  x: number;
  y: number;
};
export type GraphModel = {
  el: HTMLElement;
  data: GraphData | null;
  state: "loading" | "ready" | "empty" | "error";
  words: string;
  text: string;
  art: Node | string | null;
  selected: Set<string>;
  hidden: Set<string>;
  hover: string | null;
  focus: string | null;
  drawnWidth: number;
  bar: HTMLElement;
  showAll: HTMLButtonElement;
  legend: HTMLElement;
  box: HTMLElement;
  svg: SVGSVGElement;
  note: HTMLElement;
  plate: HTMLElement;
  drawn: boolean;
  hint: HTMLElement;
  decorate?: (part: Element, info: GraphDecorateInfo) => void;
  strings: import("./strings.js").Strings;
  labels: DrawnLabel[];
  size: [number, number];
  options: GraphOptions;
  world: SVGGElement | null;
  at: Map<
    string,
    {
      x: number;
      y: number;
      angle: number;
    }
  >;
  view: GraphView;
  zoomBox: HTMLElement;
  zoomIn: HTMLButtonElement;
  zoomOut: HTMLButtonElement;
  zoomReset: HTMLButtonElement;
  panel: HTMLElement;
  detail: string | null;
  detailShown: string | null;
  detailHook?: (info: GraphDetailInfo) => Node | string | null | void;
  readyHeight: number;
  readyWidth: number;
};
/**
 * Give a graph its nodes, edges and kinds. Ids that stay keep their
 * selection and focus; picked ids that went are dropped (and said so with
 * `kp-graph-change`).
 * @param {Element} el
 * @param {GraphData} data
 */
export declare function setGraphData(el: Element, data: GraphData): void;
/**
 * `loading`, `empty` or `error`, with the sentence to show in the picture at
 * its final height (loading says `graphLoading` when given none). Loading is the ghost of the coming network, the register's own loading picture; empty
 * and error are the package's state plate over it (`words` is its heading, `text` its sentence, `art` a picture over the
 * heading), an error with a Retry that fires `kp-graph-retry` [state-plates-1019].
 * @param {Element} el
 * @param {'loading' | 'empty' | 'error'} state
 * @param {string} [words]
 * @param {{ text?: string, art?: Node | string | null }} [plate]
 */
export declare function setGraphState(
  el: Element,
  state: "loading" | "empty" | "error",
  words?: string,
  {
    text,
    art,
  }?: {
    text?: string;
    art?: Node | string | null;
  },
): void;
/** Pick these nodes (and only these). With the details option on, the last one's details open. @param {Element} el @param {string[]} ids */
export declare function graphSelect(el: Element, ids: string[]): void;
/** Hide or show one kind of link. @param {Element} el @param {string} kind @param {boolean} hide */
export declare function graphHideKind(
  el: Element,
  kind: string,
  hide: boolean,
): void;
/**
 * Open the details of a node, or close them with `null` (the node need not be picked) [graph-1020]. It does nothing unless the
 * graph has `data-kp-detail`.
 * @param {Element} el
 * @param {string | null} id
 */
export declare function graphDetail(el: Element, id: string | null): void;
/**
 * Zoom the view from the page: `in`, `out` or `reset` [graph-1020]. It does nothing unless the graph has `data-kp-zoom`.
 * @param {Element} el
 * @param {'in' | 'out' | 'reset'} action
 */
export declare function graphZoom(
  el: Element,
  action: "in" | "out" | "reset",
): void;
/**
 * Build every graph under `root`: a bar with the kinds and Show all, the
 * picture (one tab stop: arrows between nodes, Enter or Space picks, Esc
 * shows all), and the hint under it in the page's flow. A graph's data is the
 * page's: a `script[type="application/json"][data-kp-graph-data]` child, or
 * `setGraphData()` after the attach; until then it shows loading. The options
 * are attributes on the graph (see the top of this file); `detail` is the hook
 * that fills a picked node's details panel.
 * @param {ParentNode} [root]
 * @param {{ decorate?: (part: Element, info: GraphDecorateInfo) => void, strings?: Partial<import('./strings.js').Strings>,
 *   detail?: (info: GraphDetailInfo) => Node | string | null | void }} [options]
 * @returns {() => void} detach
 */
export declare function attachGraphs(
  root?: ParentNode,
  {
    decorate,
    strings,
    detail,
  }?: {
    decorate?: (part: Element, info: GraphDecorateInfo) => void;
    strings?: Partial<import("./strings.js").Strings>;
    detail?: (info: GraphDetailInfo) => Node | string | null | void;
  },
): () => void;
