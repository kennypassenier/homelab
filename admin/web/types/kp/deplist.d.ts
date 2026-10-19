/** The lists this module draws. */
export declare const DEPLIST = "[data-kp-deplist]";
/** Fired on the list when its Retry is pressed. */
export declare const DEPLIST_RETRY_EVENT = "kp-deplist-retry";
/** How many rows show before "Show all". */
export declare const DEPLIST_COLLAPSE = 8;
export type DepItem = {
  /**
   * the thing that needs
   */
  from: string;
  /**
   * the thing needed
   */
  to: string;
  /**
   * a word for the link between them
   */
  label?: string;
  /**
   * the state of the needed thing
   */
  state?: "running" | "stopped" | "unknown";
};
export type DepRow = {
  item: DepItem;
  /**
   * its place in the order things must go (null when unnumbered)
   */
  number: number | null;
};
/**
 * @typedef {object} DepItem
 * @property {string} from the thing that needs
 * @property {string} to the thing needed
 * @property {string} [label] a word for the link between them
 * @property {'running' | 'stopped' | 'unknown'} [state] the state of the needed thing
 */
/**
 * @typedef {object} DepRow
 * @property {DepItem} item
 * @property {number | null} number its place in the order things must go (null when unnumbered)
 */
/**
 * The rows in the order things must go: a row stands after the rows of the things it needs (a thing's depth is one more than the
 * deepest thing it needs that is itself listed as needing something), numbered from 1. Rows in a cycle are not ordered: they
 * come back as `cycles`, each a path of names.
 *
 * @param {readonly DepItem[]} items
 * @param {{ numbers?: boolean }} [options]
 * @returns {{ rows: DepRow[], cycles: string[][] }}
 */
export declare function orderDependencies(
  items: readonly DepItem[],
  {
    numbers,
  }?: {
    numbers?: boolean;
  },
): {
  rows: DepRow[];
  cycles: string[][];
};
/** The rows as plain text, one per line: "paperless -> postgres". @param {readonly DepItem[]} items */
export declare const depText: (items: readonly DepItem[]) => string;
export type DeplistHandle = {
  element: HTMLElement;
  /**
   * draw the rows (clears any state)
   */
  set: (items: readonly DepItem[]) => void;
  setState: (
    state: "ready" | "loading" | "empty" | "error",
    options?: {
      message?: string;
    },
  ) => void;
  /**
   * the rows as drawn
   */
  rows: () => DepRow[];
};
/**
 * The handle of a list.
 * @param {Element} el
 * @returns {DeplistHandle | null}
 */
export declare function deplist(el: Element): DeplistHandle | null;
/**
 * Give every dependency list under `root` its rows and its states; fill one with `setDeplist` or the handle.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachDeplists(root?: ParentNode): () => void;
/**
 * Draw the rows of a list (attaching it first when it was not yet).
 * @param {HTMLElement} el
 * @param {readonly DepItem[]} items
 */
export declare function setDeplist(
  el: HTMLElement,
  items: readonly DepItem[],
): void;
