export type Tile = {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
};
/** @typedef {{ id: string, x: number, y: number, w: number, h: number }} Tile */
/** Fired on every move or resize. A contract value [TH26]: the detail is the whole layout. */
export declare const LAYOUT_EVENT = "kp-grid-layout";
/** Fired once a burst of changes settles (the debounce is `data-kp-commit-ms`). Same detail. */
export declare const LAYOUT_COMMIT_EVENT = "kp-grid-commit";
/** Columns when the grid does not say. An operational knob. */
export declare const COLUMNS = 6;
/** How long after the last change the commit event fires. */
export declare const COMMIT_MS = 400;
/**
 * @param {HTMLElement} grid
 * @returns {Tile[]}
 */
export declare function layoutOf(grid: HTMLElement): Tile[];
/**
 * Put a stored layout back on the tiles. Tiles the layout does not name
 * keep their place; ids the grid does not have are ignored.
 *
 * @param {HTMLElement} grid
 * @param {readonly Tile[]} layout
 */
export declare function applyLayout(
  grid: HTMLElement,
  layout: readonly Tile[],
): void;
export type GridHandle = {
  element: HTMLElement;
  layout: () => Tile[];
  apply: (layout: readonly Tile[]) => void;
  /**
   * place tiles added after attach
   */
  refresh: () => void;
};
/** The handle for an attached grid. @param {Element} element */
export declare function grid(element: Element): GridHandle | null;
/**
 * @param {ParentNode} root
 * @param {{ step?: number, rows?: number, commitMs?: number, pointer?: boolean }} [options]
 *   Defaults; each also per grid: `data-kp-step`, `data-kp-rows`, `data-kp-commit-ms`, `data-kp-pointer="false"`.
 * @returns {(() => void) & { handles: GridHandle[] }} detach
 */
export declare function attachGrids(
  root?: ParentNode,
  {
    step,
    rows,
    commitMs,
    pointer,
  }?: {
    step?: number;
    rows?: number;
    commitMs?: number;
    pointer?: boolean;
  },
): (() => void) & {
  handles: GridHandle[];
};
