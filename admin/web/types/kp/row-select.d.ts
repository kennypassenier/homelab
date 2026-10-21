/** An element that gets the keys whether or not it is a data table. */
export declare const ROW_SELECT = "[data-kp-select-keys]";
/** A data table that carries a select column gets them too. */
export declare const ROW_SELECT_AUTO =
  "[data-kp-datatable]:has([data-kp-select-row])";
/** The boxes this module sets. */
export declare const SELECT_BOX = "[data-kp-select-row]";
/** Fired on the element after this module changed the ticks: `{ keys, count, source, skipped }`. */
export declare const ROW_SELECT_EVENT = "kp-rows-select";
/** Fired on a row when Enter is pressed on the row itself, the event js/actions.js' row keys fire too: `{ row }`. */
export declare const ROW_OPEN_EVENT = "kp-row-open";
/** The cursor row. */
export declare const CURSOR_CLASS = "is-cursor";
/** How long a finger rests on a row before selection mode starts [ms]. */
export declare const LONG_PRESS_MS = 500;
export type RowSelectOptions = {
  range?: "shift" | "off";
  arrows?: boolean;
  allKey?: boolean;
  scroll?: "nearest" | "center";
};
export type RowSelectHandle = {
  /**
   * the keys of every ticked row
   */
  keys: () => string[];
  /**
   * the key of the cursor row
   */
  cursor: () => string | null;
  /**
   * put the cursor on a row, or take it away
   */
  moveTo: (key: string | null) => void;
  /**
   * untick everything
   */
  clear: () => void;
};
/**
 * @typedef {object} RowSelectHandle
 * @property {() => string[]} keys the keys of every ticked row
 * @property {() => string | null} cursor the key of the cursor row
 * @property {(key: string | null) => void} moveTo put the cursor on a row, or take it away
 * @property {() => void} clear untick everything
 */
/**
 * The keys of every ticked row under `el`.
 * @param {Element} el
 * @returns {string[]}
 */
export declare function rowSelectKeys(el: Element): string[];
/**
 * The handle of an element that has the keys.
 * @param {Element} el
 * @returns {RowSelectHandle | null}
 */
export declare function rowSelect(el: Element): RowSelectHandle | null;
/**
 * Give every table and list under `root` that has select boxes the keys.
 * @param {ParentNode} [root]
 * @param {RowSelectOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachRowSelect(
  root?: ParentNode,
  options?: RowSelectOptions,
): () => void;
