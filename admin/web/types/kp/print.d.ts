/** A part this module looks after. */
export declare const PRINT_ACTION = "[data-kp-print-action]";
/** The lists whose sheet this module dresses. */
export declare const PRINT_SHEETS =
  "[data-kp-checklist], [data-kp-print-sheet]";
/** Fired on the document just before the sheet is built: `{ sheets }`. */
export declare const PRINT_EVENT = "kp-print-prepare";
export type PrintOptions = {
  /**
   * the clock (a test fixes it)
   */
  now?: () => number;
  timeZone?: string;
};
/**
 * @typedef {object} PrintOptions
 * @property {() => number} [now] the clock (a test fixes it)
 * @property {string} [timeZone]
 */
/**
 * Put in the words a sheet needs, and return what takes them out again.
 * @param {Document} doc @param {PrintOptions} [options]
 * @returns {() => void}
 */
export declare function preparePrint(
  doc: Document,
  { now, timeZone }?: PrintOptions,
): () => void;
/**
 * Wire the Print buttons and the before/after print hooks of the page.
 * @param {ParentNode} [root]
 * @param {PrintOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachPrint(
  root?: ParentNode,
  options?: PrintOptions,
): () => void;
