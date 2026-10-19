// The kp-themes module chassis-rs serves at /static/kp/js/steplist.js (kp-themes 11.0.0),
// copied from kp-themes js/steplist.d.ts: chassis serves the module, not its types.
/** The lists this module looks after. */
export declare const STEPLIST_SELECTOR = ".kp-steplist";
/** The attribute that names a step, for setStep(). */
export declare const STEP_ID_ATTRIBUTE = "data-kp-step-id";
/** Fired on the list, bubbling, when a step changed state after attach: `{ item, id, state, previous }`. */
export declare const STEPLIST_EVENT = "kp-steplist-step";
/** The states a step can be in. */
export declare const STEP_STATES: readonly [
  "pending",
  "running",
  "done",
  "failed",
  "skipped",
  "current",
];
export type StepState =
  "pending" | "running" | "done" | "failed" | "skipped" | "current";
export type StepRow = {
  /**
   * names the step for setStep(); defaults to its index
   */
  id?: string;
  title: string;
  /**
   * the one-line description under the title; give every step one, so a note arriving later moves nothing
   */
  note?: string;
  /**
   * pending when absent
   */
  state?: StepState;
  /**
   * how long it took, as the page prints it (in a journey: the date the step was reached)
   */
  time?: string;
  /**
   * the step's own colour, `--chart-1` … `--chart-5` (`data-kp-tone`) [kpadopt4]
   */
  tone?: 1 | 2 | 3 | 4 | 5;
};
/**
 * Build a step list from rows. The result still needs attachSteplists() (on
 * the element or its container) for the spoken parts.
 *
 * @param {StepRow[]} rows
 * @param {{ label?: string }} [options] `label`: the list's accessible name
 * @returns {HTMLOListElement}
 */
export declare function buildSteplist(
  rows: StepRow[],
  {
    label,
  }?: {
    label?: string;
  },
): HTMLOListElement;
/**
 * Change one step. `key` is the step's `data-kp-step-id` or its index.
 *
 * @param {Element} list
 * @param {string | number} key
 * @param {{ state?: StepState, title?: string, note?: string, time?: string }} change
 * @returns {Element | null} the item, or null when there is no such step
 */
export declare function setStep(
  list: Element,
  key: string | number,
  change: {
    state?: StepState;
    title?: string;
    note?: string;
    time?: string;
  },
): Element | null;
/**
 * Sync every `.kp-steplist` under `root`, and follow `data-state` changes
 * later, so a framework or a hand-written `setAttribute` is followed too.
 * A list rendered after this ran is attached by calling it on that subtree.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach: stops following the lists
 */
export declare function attachSteplists(root?: ParentNode): () => void;
