/** A rating. */
export declare const RATING = "[data-kp-rating]";
/** Fired on the rating when a person changes it: `{ value, previous, source, wait }`. */
export declare const RATING_EVENT = "kp-rating-change";
/** Fired on the rating when Retry is pressed on a rating that could not be read. */
export declare const RATING_RETRY_EVENT = "kp-rating-retry";
/** The states a rating can be in. */
export declare const RATING_STATES: readonly ["ready", "loading", "error"];
export type RatingSource = "pointer" | "keyboard" | "clear" | "undo";
export type RatingModel = {
  el: HTMLElement;
  max: number;
  step: 1 | 0.5;
  mode: "input" | "display";
  value: number;
  /**
   * the pick held back by commit confirm, else the value
   */
  draft: number;
  preview: number | null;
  labels: string[];
  row: HTMLElement;
  stars: HTMLElement[];
  caption: HTMLElement;
  reason: HTMLElement;
  flash: HTMLElement;
  live: HTMLElement;
  failure: HTMLElement;
  actions: HTMLElement;
  field: HTMLInputElement | null;
  busy: boolean;
  flashTimer: number;
};
/** A value snapped to the step and the scale. Pure. @param {number} value @param {number} max @param {number} step */
export declare function snapRating(
  value: number,
  max: number,
  step?: number,
): number;
export type RatingOptions = {
  /**
   * the scale, 3 to 10 (default 5)
   */
  max?: number;
  /**
   * whole or half stars (default 1)
   */
  step?: 1 | 0.5;
  /**
   * a row to set, or one to read (default input)
   */
  mode?: "input" | "display";
};
/**
 * @typedef {object} RatingOptions
 * @property {number} [max] the scale, 3 to 10 (default 5)
 * @property {1 | 0.5} [step] whole or half stars (default 1)
 * @property {'input' | 'display'} [mode] a row to set, or one to read (default input)
 */
/**
 * Draw and wire every rating under `root`.
 * @param {ParentNode} [root]
 * @param {RatingOptions} [options] defaults; each element's own `data-kp-rating-*` wins
 * @returns {() => void} detach
 */
export declare function attachRatings(
  root?: ParentNode,
  options?: RatingOptions,
): () => void;
export type RatingHandle = {
  element: HTMLElement;
  /**
   * 0 is not rated
   */
  value: () => number;
  /**
   * replace the value (no event)
   */
  set: (value: number) => void;
  setState: (state: "ready" | "loading" | "error") => void;
  focus: () => void;
};
/**
 * @typedef {object} RatingHandle
 * @property {HTMLElement} element
 * @property {() => number} value 0 is not rated
 * @property {(value: number) => void} set replace the value (no event)
 * @property {(state: 'ready' | 'loading' | 'error') => void} setState
 * @property {() => void} focus
 */
/**
 * The handle of an attached rating.
 * @param {Element} element
 * @returns {RatingHandle | null}
 */
export declare function rating(element: Element): RatingHandle | null;
/**
 * Set the value from a script, without the event (the page read it from somewhere).
 * @param {Element} element @param {number} value 0 clears
 */
export declare function setRating(element: Element, value: number): void;
/**
 * The state: `loading` (dashed stars that pulse, the row's size kept), `error` (the stars replaced at the same size by
 * "Could not read the rating" and Retry), `ready`.
 * @param {Element} element @param {'ready' | 'loading' | 'error'} next
 */
export declare function setRatingState(
  element: Element,
  next: "ready" | "loading" | "error",
): void;
/**
 * A rating inside a data table cell: the display row, and the number the table sorts and filters the cell by.
 * @param {HTMLElement} cell the `td` (or any element inside one)
 * @param {number} value 0 is not rated
 * @param {{ max?: number, count?: number, for?: string }} [options]
 * @returns {HTMLElement} the rating element
 */
export declare function ratingCell(
  cell: HTMLElement,
  value: number,
  {
    max,
    count,
    for: item,
  }?: {
    max?: number;
    count?: number;
    for?: string;
  },
): HTMLElement;
