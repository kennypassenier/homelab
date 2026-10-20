// The kp-themes module chassis-rs serves at /static/kp/js/runs.js (kp-themes 11.1.1),
// copied from kp-themes js/runs.d.ts: chassis serves the module, not its types.
/** A next-times list. */
export declare const RUNS = "[data-kp-runs]";
/** Fired on the list, bubbling, when Try again is pressed: `{ element }`. */
export declare const RUNS_RETRY_EVENT = "kp-runs-retry";
/** The states a list can be in. */
export declare const RUNS_STATES: readonly [
  "ready",
  "empty",
  "loading",
  "error",
];
export type RunInput =
  | number
  | string
  | Date
  | {
      at: number | string | Date;
      conflict?: string;
    };
export type Run = {
  /**
   * ms
   */
  at: number;
  conflict?: string;
};
export type RunsState = "ready" | "empty" | "loading" | "error";
export type RunsOptions = {
  /**
   * the sentence an empty list shows ("Pick a time")
   */
  reason?: string;
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
 * @typedef {number | string | Date | { at: number | string | Date, conflict?: string }} RunInput
 * @typedef {object} Run
 * @property {number} at ms
 * @property {string} [conflict]
 * @typedef {'ready' | 'empty' | 'loading' | 'error'} RunsState
 * @typedef {object} RunsOptions
 * @property {string} [reason] the sentence an empty list shows ("Pick a time")
 * @property {string} [title] an error's first line
 * @property {string} [text] an error's sentence
 */
/**
 * A moment's distance from now in the coarsest honest words: "in 35 min", "in 3 h 20 min", "in 2 days". Pure.
 * @param {number} ms the moment
 * @param {number} now
 * @param {import('./strings.js').Strings} [strings]
 */
export declare function runsRelative(
  ms: number,
  now: number,
  strings?: import("./strings.js").Strings,
): string;
/**
 * How far a zone's wall clock runs ahead of UTC at a moment, in minutes. Pure.
 * @param {number} ms @param {string} zone
 */
export declare function zoneOffset(ms: number, zone?: string): number;
/**
 * A run as rule 52 writes it with its weekday: "Mon 20/10/2026 03:00". Pure.
 * @param {number} ms @param {string} [zone] @param {string} [locale]
 */
export declare function runMoment(
  ms: number,
  zone?: string,
  locale?: string,
): string;
/**
 * The hours the clock was turned between two runs: positive when it went forward. 0 when it was not. Pure.
 * @param {number} before @param {number} after @param {string} [zone]
 */
export declare function clockShift(
  before: number,
  after: number,
  zone?: string,
): number;
export type RunsHandle = {
  element: HTMLElement;
  /**
   * the times; none is the empty state
   */
  set: (times: readonly RunInput[], options?: RunsOptions) => void;
  setState: (state: RunsState, options?: RunsOptions) => void;
  runs: () => Run[];
  state: () => RunsState;
};
/** The handle of an attached list. @param {Element} element */
export declare function runs(element: Element): RunsHandle | null;
/**
 * Give a list its times (none: the empty state, with `reason`).
 * @param {Element} element @param {readonly RunInput[]} times @param {RunsOptions} [options]
 */
export declare function setRuns(
  element: Element,
  times: readonly RunInput[],
  options?: RunsOptions,
): void;
/**
 * Put a list in a state: ready, empty, loading or error.
 * @param {Element} element @param {RunsState} state @param {RunsOptions} [options]
 */
export declare function setRunsState(
  element: Element,
  state: RunsState,
  options?: RunsOptions,
): void;
/**
 * Wire every next-times list under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number }} [options]
 * @returns {() => void} detach
 */
export declare function attachRuns(
  root?: ParentNode,
  {
    now: nowOption,
  }?: {
    now?: () => number;
  },
): () => void;
