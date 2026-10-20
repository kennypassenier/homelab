// The kp-themes module chassis-rs serves at /static/kp/js/barcell.js (kp-themes 11.1.1),
// copied from kp-themes js/barcell.d.ts: chassis serves the module, not its types.
/** A bar cell written in markup. */
export declare const BAR_CELL = "[data-kp-bar]";
/** The attribute a data table sorts a cell by. */
export declare const SORT_VALUE_ATTRIBUTE = "data-kp-sort-value";
export type BarCell = {
  /**
   * the share, 0 up (1 is full); null is not measured
   */
  value?: number | null;
  /**
   * the number beside the bar ("62%", "2.1 GB"); default the rounded percentage, or empty
   */
  text?: string;
  /**
   * the fill's colour; omit to let `warnAt` and `badAt` decide, null for none
   */
  tone?: "warning" | "destructive" | null;
  /**
   * from this share the fill is the warning colour (default 0.75)
   */
  warnAt?: number;
  /**
   * from this share the fill is the destructive colour (default 0.9)
   */
  badAt?: number;
  /**
   * what a data table sorts and filters this cell by; default the share as a percentage, '' when not measured
   */
  sortValue?: number | string | null;
  /**
   * a tick on the bar (a limit, a target)
   */
  mark?: number | null;
  /**
   * what the share is, for a screen reader ("full")
   */
  label?: string;
  /**
   * what the mark is
   */
  markLabel?: string;
  loading?: boolean;
};
/**
 * @typedef {object} BarCell
 * @property {number | null} [value]  the share, 0 up (1 is full); null is not measured
 * @property {string} [text]          the number beside the bar ("62%", "2.1 GB"); default the rounded percentage, or empty
 * @property {'warning' | 'destructive' | null} [tone]  the fill's colour; omit to let `warnAt` and `badAt` decide, null for none
 * @property {number} [warnAt]        from this share the fill is the warning colour (default 0.75)
 * @property {number} [badAt]         from this share the fill is the destructive colour (default 0.9)
 * @property {number | string | null} [sortValue]  what a data table sorts and filters this cell by; default the share as a percentage, '' when not measured
 * @property {number | null} [mark]   a tick on the bar (a limit, a target)
 * @property {string} [label]         what the share is, for a screen reader ("full")
 * @property {string} [markLabel]     what the mark is
 * @property {boolean} [loading]
 */
/**
 * Write a bar cell: the meter, the number, and the sort value on the cell
 * around it.
 *
 * @param {HTMLElement} el  the `.kp-bar-cell` (made one when it is not)
 * @param {BarCell} cell
 */
export declare function setBarCell(
  el: HTMLElement,
  {
    value,
    text,
    tone,
    warnAt,
    badAt,
    sortValue,
    mark,
    label,
    markLabel,
    loading,
  }: BarCell,
): void;
/**
 * Fill every `[data-kp-bar]` under `root` and keep it filled when its
 * attributes change.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachBarCells(root?: ParentNode): () => void;
