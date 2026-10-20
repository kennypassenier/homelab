/** The lines view of a matrix. */
export declare const USES_MAP = '[data-kp-view="lines"]';
/** Fired on the matrix when a node was pinned or let go: `{ key, side, pinned, related }`. */
export declare const USES_PIN_EVENT = "kp-uses-pin";
/** Fired on the matrix when the Retry of a failed map was pressed. */
export declare const USES_RETRY_EVENT = "kp-uses-retry";
/** The narrowest the map draws lines at, in px. */
export declare const USES_COLLAPSE_BELOW = 640;
export type UsesNode = {
  key: string;
  label: string;
  /**
   * a line under the label
   */
  sub?: string;
  /**
   * a document mark before the label
   */
  glyph?: "pdf" | "doc" | "sheet" | "image" | "text" | "unknown";
  /**
   * a link to the thing itself
   */
  href?: string;
  /**
   * `ok` `warn` `bad` `muted`
   */
  tone?: string;
  /**
   * not a real thing but still a group (a faint node)
   */
  ghost?: boolean;
  /**
   * its lines are dashed
   */
  dashed?: boolean;
  /**
   * the node cannot be opened, and why
   */
  disabledReason?: string;
  /**
   * kept from a matrix row: a colour
   */
  mark?: string;
};
export type UsesLine = {
  /**
   * the line's colour (and its pattern)
   */
  tone?: "ok" | "warn" | "bad" | "muted";
  /**
   * the words on the line
   */
  label?: string;
};
/** The view a matrix remembers on this device, or null. @param {Element} el @returns {'grid' | 'lines' | null} */
export declare function rememberedView(el: Element): "grid" | "lines" | null;
/** Remember the view a matrix shows. @param {Element} el @param {'grid' | 'lines'} view */
export declare function rememberView(el: Element, view: "grid" | "lines"): void;
/**
 * The relation of a spec, as lists. Pure.
 * @param {{ rows?: readonly UsesNode[], columns?: readonly UsesNode[], cell?: (row: string, column: string) => unknown }} spec
 * @returns {{ uses: Map<string, string[]>, usedBy: Map<string, string[]>, lines: Map<string, UsesLine> }} `lines` by `"<row>><column>"`
 */
export declare function usesRelation(spec: {
  rows?: readonly UsesNode[];
  columns?: readonly UsesNode[];
  cell?: (row: string, column: string) => unknown;
}): {
  uses: Map<string, string[]>;
  usedBy: Map<string, string[]>;
  lines: Map<string, UsesLine>;
};
/**
 * The path of one line between two points. Pure.
 * @param {'curve' | 'elbow' | 'straight'} shape @param {number} x1 @param {number} y1 @param {number} x2 @param {number} y2
 */
export declare function usesPath(
  shape: "curve" | "elbow" | "straight",
  x1: number,
  y1: number,
  x2: number,
  y2: number,
): string;
export type Context = {
  replaceKeepingCard: (el: HTMLElement, nodes: Node[]) => void;
  syncCard: (
    el: HTMLElement,
    loading: boolean,
    words: string,
    area: () => readonly (
      | Element
      | {
          top: number;
          right: number;
          bottom: number;
          left: number;
        }
    )[],
    className: string,
  ) => void;
  markDrawn: (el: HTMLElement) => void;
};
/**
 * Draw the lines view of a matrix.
 *
 * @param {HTMLElement} el
 * @param {any} spec a MatrixSpec with the lines fields
 * @param {Context} ctx
 */
export declare function drawUsesMap(
  el: HTMLElement,
  spec: any,
  ctx: Context,
): void;
/**
 * Pin a node (or none) from a script, without a page event beyond `kp-uses-pin`.
 * @param {HTMLElement} el @param {string | null} key
 */
export declare function setUsesPin(el: HTMLElement, key: string | null): void;
/**
 * Take the lines view off a matrix that goes back to its grid.
 * @param {HTMLElement} el
 */
export declare function leaveUsesMap(el: HTMLElement): void;
