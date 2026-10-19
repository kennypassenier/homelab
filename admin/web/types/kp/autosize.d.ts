/** A box that grows. */
export declare const AUTOSIZE = "textarea[data-kp-autosize]";
/** Fired on the box when its height changed: `{ height, lines, capped }`. */
export declare const AUTOSIZE_EVENT = "kp-autosize-resize";
/** The most lines a box grows to unless it says otherwise. */
export declare const AUTOSIZE_MAX_LINES = 20;
/** The least lines. */
export declare const AUTOSIZE_MIN_LINES = 3;
/** The states a box can be in. */
export declare const AUTOSIZE_STATES: readonly ["ready", "loading", "error"];
/**
 * The line height in px of a box: its computed value, else 1.2 times its font size (`normal`). Pure over a computed style.
 * @param {CSSStyleDeclaration} cs
 */
export declare function lineHeightOf(cs: CSSStyleDeclaration): number;
export type FitOptions = {
  /**
   * the least lines (default 3)
   */
  minRows?: number;
  /**
   * the most lines; null: no cap (default 20)
   */
  maxRows?: number | null;
  /**
   * a second cap in px (a phone's visible height)
   */
  maxPx?: number;
  /**
   * never shorter than this (shrink off: the tallest the box has been)
   */
  keepPx?: number;
  /**
   * write the height (default true)
   */
  apply?: boolean;
};
/**
 * @typedef {object} FitOptions
 * @property {number} [minRows] the least lines (default 3)
 * @property {number | null} [maxRows] the most lines; null: no cap (default 20)
 * @property {number} [maxPx] a second cap in px (a phone's visible height)
 * @property {number} [keepPx] never shorter than this (shrink off: the tallest the box has been)
 * @property {boolean} [apply] write the height (default true)
 */
/**
 * The height a box needs for its text, between the least and the most lines. Measured on a twin, so it is exact and the page
 * does not move.
 *
 * @param {HTMLTextAreaElement} area
 * @param {FitOptions} [options]
 * @returns {{ height: number, scrolls: boolean, capped: boolean, lines: number, lineHeight: number }}
 */
export declare function fitTextarea(
  area: HTMLTextAreaElement,
  { minRows, maxRows, maxPx, keepPx, apply }?: FitOptions,
): {
  height: number;
  scrolls: boolean;
  capped: boolean;
  lines: number;
  lineHeight: number;
};
export type AutosizeOptions = {
  /**
   * the least lines
   */
  rows?: number;
  /**
   * the most lines
   */
  max?: number | "none";
  engine?: "css" | "js" | "auto";
  shrink?: boolean;
  counter?: "off" | "chars" | "lines" | "both";
};
export type AutosizeHandle = {
  element: HTMLTextAreaElement;
  /**
   * measure again (after a value written by a script that bypassed the setter)
   */
  refresh: () => void;
  setState: (state: "ready" | "loading" | "error", message?: string) => void;
  count: () => {
    chars: number;
    lines: number;
  };
  /**
   * the engine in use
   */
  engine: "css" | "js";
};
/** The handle of an attached box. @param {Element} element */
export declare function autosize(element: Element): AutosizeHandle | null;
/**
 * Measure a box again.
 * @param {Element} element
 */
export declare function refreshAutosize(element: Element): void;
/**
 * Put a box in a state: `loading` lays the busy card over it, `error` writes the sentence in its error line.
 * @param {Element} element @param {'ready' | 'loading' | 'error'} state @param {string} [message]
 */
export declare function setAutosizeState(
  element: Element,
  state: "ready" | "loading" | "error",
  message?: string,
): void;
/** Whether this browser sizes a field to its content itself. */
export declare function cssAutosizeSupported(): boolean;
/**
 * Wire every growing box under `root`.
 * @param {ParentNode} [root]
 * @param {AutosizeOptions} [options] overrides the markup
 * @returns {() => void} detach
 */
export declare function attachAutosize(
  root?: ParentNode,
  options?: AutosizeOptions,
): () => void;
