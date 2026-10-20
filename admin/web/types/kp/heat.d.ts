// The kp-themes module chassis-rs serves at /static/kp/js/heat.js (kp-themes 11.1.1),
// copied from kp-themes js/heat.d.ts: chassis serves the module, not its types.
/** A heat grid. */
export declare const HEAT = "[data-kp-heat]";
/** A matrix. */
export declare const MATRIX = "[data-kp-matrix]";
/** Fired on a heat grid when a column is picked or dropped. */
export declare const HEAT_PICK_EVENT = "kp-heat-pick";
/** Fired on a heat grid when a row label is pressed. */
export declare const HEAT_ROW_EVENT = "kp-heat-row";
/** Fired on a heat grid on Page Up and Page Down. */
export declare const HEAT_PAGE_EVENT = "kp-heat-page";
/** Fired on a matrix when a cell is pinned. */
export declare const MATRIX_PICK_EVENT = "kp-matrix-pick";
/** Fired on a heat grid, bubbling, when the Retry of its failed state is pressed [state-plates-1019]. */
export declare const HEAT_RETRY_EVENT = "kp-heat-retry";
/** Fired on a matrix (either view), bubbling, when the Retry of its failed state is pressed [state-plates-1019]. */
export declare const MATRIX_RETRY_EVENT = "kp-matrix-retry";
/** Fired on a matrix when the lit headers change. */
export declare const MATRIX_LIGHT_EVENT = "kp-matrix-light";
export type GridTone =
  | "ok"
  | "warn"
  | "bad"
  | "muted"
  | "future"
  | "before"
  | "none"
  | "level"
  | "loading"
  | "self"
  | "logged"
  | "quiet"
  | "paused";
export type HeatCell = {
  /**
   * default `muted`
   */
  tone?: GridTone;
  /**
   * 0 to 1, for tone `level`: the darker, the more
   */
  level?: number;
  /**
   * 0 to 1, summary row only: how much of the cell the tone fills, from the bottom
   */
  share?: number;
  /**
   * what the cell says in words ("backed up", "missed"); the row and the column are put in front of it
   */
  label: string;
  /**
   * the whole accessible name, when the default is not right
   */
  aria?: string;
  /**
   * the hover card's text; `true` leaves it to a `fill` the page gave attachHoverCards()
   */
  tip?: string | true;
  /**
   * a cell that cannot be pressed
   */
  disabled?: boolean;
};
export type HeatRow = {
  key: string;
  label: string;
  /**
   * a CSS colour: a small square before the label
   */
  mark?: string;
  /**
   * the label is quiet (a row that keeps no data)
   */
  muted?: boolean;
  /**
   * the label is pressed (when labels are toggles)
   */
  pressed?: boolean;
  /**
   * one per column
   */
  cells: readonly HeatCell[];
};
export type HeatSpec = {
  /**
   * `minor: true` draws no name, `minor: false` always does
   */
  columns?: readonly {
    key: string;
    label: string;
    minor?: boolean;
  }[];
  summary?: HeatRow;
  rows?: readonly HeatRow[];
  /**
   * default ready
   */
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the words of the empty and error states (the plate's heading)
   */
  message?: string;
  /**
   * the plate's sentence under the heading (an error says the package's own when none is given)
   */
  text?: string;
  /**
   * a picture over the heading of the empty and error plates
   */
  art?: Node | string | null;
  /**
   * the shape of the loading state when there is no data yet
   */
  skeleton?: {
    rows: number;
    columns: number;
  };
  /**
   * every how many columns the axis names one (default 3; the last always); columns with `minor` set do not count
   */
  every?: number;
  /**
   * where the column axis stands (default start)
   */
  axis?: "start" | "end";
  /**
   * row labels are buttons that toggle (else `data-kp-heat-toggle` on the grid)
   */
  toggle?: boolean;
  /**
   * the key of the picked column
   */
  picked?: string | null;
  /**
   * the grid's accessible name, when the element has none
   */
  label?: string;
  /**
   * the legend under the grid (setHeatLegend); `false` takes it away
   */
  legend?: readonly HeatLegendEntry[] | false;
  /**
   * the key line at the end of the legend
   */
  legendKeys?: HeatLegendOptions["keys"];
};
export type HeatLegendEntry = {
  /**
   * a swatch of this tone (with `label`)
   */
  tone?: GridTone;
  /**
   * its words
   */
  label?: string;
  /**
   * instead of a tone: the value scale, `less` then `steps` swatches of rising level then `more`
   */
  ramp?: boolean;
  /**
   * how many swatches the ramp has (default 5, 2 to 9)
   */
  steps?: number;
  /**
   * the ramp's low end (default "Less")
   */
  less?: string;
  /**
   * the ramp's high end (default "More")
   */
  more?: string;
};
export type HeatLegendOptions = {
  /**
   * `true`: the keys of a heat grid (move, pick a column, drop the pick, earlier or later); or the pairs to show; at the end of the legend
   */
  keys?: boolean | readonly import("./keys.js").KeyPair[];
  /**
   * the legend's name for a screen reader (default "Legend")
   */
  label?: string;
};
export type MatrixCell = {
  tone?: GridTone;
  /**
   * what the cell shows (a count, "open"); without it a glyph for the tone stands there
   */
  text?: string;
  /**
   * what the cell says in words, for a screen reader and the card
   */
  label?: string;
  /**
   * the whole accessible name, when the default is not right
   */
  aria?: string;
  tip?: string | true;
  /**
   * a red ring on top of the tone (stopped on the way)
   */
  ringed?: boolean;
  /**
   * a dashed primary outline (what a rule being read touches)
   */
  mark?: boolean;
  disabled?: boolean;
};
export type MatrixSpec = {
  /**
   * the words in the corner ("from ↓ to →")
   */
  corner?: string;
  /**
   * the sources, in the lines view with their node content (`sub`, `glyph`, `href`, `ghost`, `dashed`)
   */
  rows?: readonly import("./uses-map.js").UsesNode[];
  /**
   * the things that use them
   */
  columns?: readonly import("./uses-map.js").UsesNode[];
  /**
   * null: unknown; the same key on both axes with nothing given: itself
   */
  cell?: (row: string, column: string) => MatrixCell | null | undefined;
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the plate's heading in the empty and error states
   */
  message?: string;
  /**
   * the plate's sentence under the heading
   */
  text?: string;
  /**
   * a picture over the heading of the empty and error plates
   */
  art?: Node | string | null;
  /**
   * the size of the loading state when there are no names yet
   */
  skeleton?: number;
  picked?: {
    row: string;
    column: string;
  } | null;
  /**
   * the header keys that are lit
   */
  lit?: readonly string[];
  label?: string;
  /**
   * `lines`: the two columns joined by lines (js/uses-map.js); default `grid`
   */
  view?: "grid" | "lines";
  /**
   * lines view: the words after the number of uses ("2 applications")
   */
  countLabel?: (count: number) => string;
  /**
   * lines view: the sources column's name for a screen reader
   */
  sourcesLabel?: string;
  /**
   * lines view: the users column's name
   */
  usersLabel?: string;
};
/**
 * Write a heat grid: the column axis, an optional summary row and the rows,
 * replacing what the element held. The grid keeps its columns, its row
 * heights and its picked column across a rewrite; a focused cell keeps its
 * place.
 *
 * @param {HTMLElement} el
 * @param {HeatSpec} spec
 */
export declare function setHeat(el: HTMLElement, spec: HeatSpec): void;
/**
 * Pick a column (or none, with `null`) without an event: the cells and the
 * axis name of that column are marked `data-kp-picked`.
 * @param {HTMLElement} el
 * @param {string | null} key
 */
export declare function setHeatPick(el: HTMLElement, key: string | null): void;
/**
 * Press the labels of these rows (and unpress the rest) without an event.
 * @param {HTMLElement} el
 * @param {readonly string[]} keys
 */
export declare function setHeatRows(
  el: HTMLElement,
  keys: readonly string[],
): void;
/**
 * The legend under a heat grid: a swatch and its words per entry, in the order given, a value ramp where an entry asks for one,
 * and at the end the line of keys. The list is the one right after the grid (made when there is none), named in the grid's
 * `aria-describedby`; no entries and no keys hides it. Called again, it rewrites in place, so the legend keeps its place while the
 * grid loads.
 *
 * @param {HTMLElement} el the heat grid
 * @param {readonly HeatLegendEntry[]} entries
 * @param {HeatLegendOptions & { source?: 'auto' }} [options]
 * @returns {HTMLElement | null} the legend, null for a grid that is not in a page
 */
export declare function setHeatLegend(
  el: HTMLElement,
  entries: readonly HeatLegendEntry[],
  options?: HeatLegendOptions & {
    source?: "auto";
  },
): HTMLElement | null;
/**
 * Wire every heat grid under `root`: pick, row labels, keys.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachHeats(root?: ParentNode): () => void;
/**
 * Write a matrix: the corner, the column headers, then a row header and a
 * cell for each column on every row. The headers are `position: sticky`.
 *
 * @param {HTMLElement} el
 * @param {MatrixSpec} spec
 */
export declare function setMatrix(el: HTMLElement, spec: MatrixSpec): void;
/**
 * Light these headers (their row and their column) and dim the rest, without an event.
 * @param {HTMLElement} el
 * @param {readonly string[]} keys
 */
export declare function setMatrixLit(
  el: HTMLElement,
  keys: readonly string[],
): void;
/**
 * Pin one cell (or none) without an event.
 * @param {HTMLElement} el
 * @param {{ row: string, column: string } | null} pick
 */
export declare function setMatrixPick(
  el: HTMLElement,
  pick: {
    row: string;
    column: string;
  } | null,
): void;
/**
 * Wire every matrix under `root`: pin, light the headers, keys.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachMatrices(root?: ParentNode): () => void;
