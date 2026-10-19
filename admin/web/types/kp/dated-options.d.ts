/** A dated option picker. */
export declare const DATED = "[data-kp-dated]";
/** Fired on the picker when an option is chosen by a person: `{ value, option }`. */
export declare const DATED_EVENT = "kp-dated-change";
/** Fired on the picker, bubbling, when Try again is pressed: `{ element }`. */
export declare const DATED_RETRY_EVENT = "kp-dated-retry";
/** The states a picker can be in. */
export declare const DATED_STATES: readonly [
  "ready",
  "empty",
  "loading",
  "error",
];
export type DatedOption = {
  /**
   * what the page gets back
   */
  value: string;
  /**
   * the option's name; the date when absent
   */
  label?: string;
  /**
   * the moment it was made
   */
  when?: number | string | Date;
  /**
   * a short text, a size or a kind, in the column at the end
   */
  meta?: string;
  /**
   * why it cannot be chosen now; the option stays and says so
   */
  disabled?: string;
};
export type DatedState = "ready" | "empty" | "loading" | "error";
export type DatedStateOptions = {
  /**
   * an empty picker's sentence ("No snapshots yet")
   */
  reason?: string;
  /**
   * a button under an empty picker's sentence ("Take a snapshot")
   */
  action?: Node | null;
  /**
   * an error's first line
   */
  title?: string;
  /**
   * an error's sentence
   */
  text?: string;
};
/**
 * @typedef {object} DatedOption
 * @property {string} value what the page gets back
 * @property {string} [label] the option's name; the date when absent
 * @property {number | string | Date} [when] the moment it was made
 * @property {string} [meta] a short text, a size or a kind, in the column at the end
 * @property {string} [disabled] why it cannot be chosen now; the option stays and says so
 * @typedef {'ready' | 'empty' | 'loading' | 'error'} DatedState
 * @typedef {object} DatedStateOptions
 * @property {string} [reason] an empty picker's sentence ("No snapshots yet")
 * @property {Node | null} [action] a button under an empty picker's sentence ("Take a snapshot")
 * @property {string} [title] an error's first line
 * @property {string} [text] an error's sentence
 */
/**
 * How long ago a moment was, in words: "just now", "35 min ago", "3 h ago", "3 days ago". Pure.
 * @param {number} ms @param {number} now @param {import('./strings.js').Strings} [strings]
 */
export declare function datedAge(
  ms: number,
  now: number,
  strings?: import("./strings.js").Strings,
): string;
/**
 * The options in the order a picker shows them. Options without a moment keep their place after those with one. Pure.
 * @param {readonly DatedOption[]} options @param {'newest' | 'oldest' | 'given'} order
 * @returns {DatedOption[]}
 */
export declare function orderDated(
  options: readonly DatedOption[],
  order?: "newest" | "oldest" | "given",
): DatedOption[];
export type DatedHandle = {
  element: HTMLElement;
  /**
   * the options; none is the empty state
   */
  set: (options: readonly DatedOption[], value?: string | null) => void;
  setState: (state: DatedState, options?: DatedStateOptions) => void;
  /**
   * the chosen value
   */
  value: () => string | null;
  /**
   * choose a value from a script (no event)
   */
  select: (value: string | null) => void;
  /**
   * the options, in the order shown
   */
  options: () => DatedOption[];
  state: () => DatedState;
};
/** The handle of an attached picker. @param {Element} element */
export declare function datedOptions(element: Element): DatedHandle | null;
/**
 * Give a picker its options (none: the empty state), and optionally the value to choose (the newest by default).
 * @param {Element} element @param {readonly DatedOption[]} options @param {string | null} [value]
 */
export declare function setDatedOptions(
  element: Element,
  options: readonly DatedOption[],
  value?: string | null,
): void;
/**
 * Put a picker in a state: ready, empty, loading or error.
 * @param {Element} element @param {DatedState} state @param {DatedStateOptions} [options]
 */
export declare function setDatedState(
  element: Element,
  state: DatedState,
  options?: DatedStateOptions,
): void;
/**
 * The value a picker holds.
 * @param {Element} element
 */
export declare function datedValue(element: Element): string | null;
/**
 * Wire every dated option picker under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number }} [options]
 * @returns {() => void} detach
 */
export declare function attachDatedOptions(
  root?: ParentNode,
  {
    now: nowOption,
  }?: {
    now?: () => number;
  },
): () => void;
