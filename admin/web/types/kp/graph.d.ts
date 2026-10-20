/** A hub-and-ring graph: `figure.kp-graph[data-kp-graph]`. */
export declare const GRAPH = "[data-kp-graph]";
/** Fired on the graph, bubbling, when its selection, hover or hidden kinds change: `detail` `{ selected, hover, hiddenKinds }`. */
export declare const GRAPH_CHANGE_EVENT = "kp-graph-change";
/** Fired on the graph, bubbling, when the Retry of its failed state is pressed [state-plates-1019]. */
export declare const GRAPH_RETRY_EVENT = "kp-graph-retry";
export type GraphNode = {
  id: string;
  label: string;
  kind?: string;
  hue?: number;
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
  kind: "node" | "kind" | "show-all";
  host: HTMLElement;
  key?: string;
  index?: number;
  label?: string;
  value?: string;
};
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
/**
 * Where every node sits in a W×H box: the hub in the middle, the others on an
 * ellipse around it (stretched sideways on a wide box). On a narrow box the
 * ellipse gives up width for height, so the labels beside the ring keep room.
 * @param {GraphData} data
 * @param {number} W
 * @param {number} H
 * @returns {Map<string, { x: number, y: number, angle: number }>}
 */
export declare function graphLayout(
  data: GraphData,
  W: number,
  H: number,
): Map<
  string,
  {
    x: number;
    y: number;
    angle: number;
  }
>;
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
/** Pick these nodes (and only these). @param {Element} el @param {string[]} ids */
export declare function graphSelect(el: Element, ids: string[]): void;
/** Hide or show one kind of link. @param {Element} el @param {string} kind @param {boolean} hide */
export declare function graphHideKind(
  el: Element,
  kind: string,
  hide: boolean,
): void;
/**
 * Build every graph under `root`: a bar with the kinds and Show all, the
 * picture (one tab stop: arrows between nodes, Enter or Space picks, Esc
 * shows all), and the hint under it in the page's flow. A graph's data is the
 * page's: a `script[type="application/json"][data-kp-graph-data]` child, or
 * `setGraphData()` after the attach; until then it shows loading.
 * @param {ParentNode} [root]
 * @param {{ decorate?: (part: Element, info: GraphDecorateInfo) => void, strings?: Partial<import('./strings.js').Strings> }} [options]
 * @returns {() => void} detach
 */
export declare function attachGraphs(
  root?: ParentNode,
  {
    decorate,
    strings,
  }?: {
    decorate?: (part: Element, info: GraphDecorateInfo) => void;
    strings?: Partial<import("./strings.js").Strings>;
  },
): () => void;
