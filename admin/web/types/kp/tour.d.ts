export type Strings = import("./strings.js").Strings;
export type TourStep = {
  target: string | (() => Element | null);
  title: string;
  text: string;
};
export type TourDecorateInfo = {
  kind: "tour-next" | "tour-back" | "tour-skip";
  host: HTMLElement;
  key?: string;
  index?: number;
  label?: string;
  value?: string;
};
export type TourOptions = {
  /**
   * the step to open on, counted from 0 among the steps that are on the page
   */
  start?: number;
  /**
   * the name a finished or skipped tour is remembered under (js/remember.js, component `tour`)
   */
  remember?: string;
  /**
   * called with the card's buttons each time a tour builds them [R-DRIVE]
   */
  decorate?: (part: HTMLElement, info: TourDecorateInfo) => void;
  /**
   * where the focus goes when the tour ends; the focused element when it started, by default
   */
  returnFocus?: HTMLElement | null;
  /**
   * called once when it ends: `true` when Done was pressed on the last step
   */
  onEnd?: (finished: boolean) => void;
  /**
   * any of the dictionary's `tour…` words, for this tour only
   */
  strings?: Partial<Strings>;
};
export type TourHandle = {
  end: () => void;
  goto: (i: number) => void;
};
/** @typedef {import('./strings.js').Strings} Strings */
/**
 * One step of a tour: the part of the page it is about (a selector, whose
 * first shown match is taken, or a function), and what the card says.
 * @typedef {object} TourStep
 * @property {string | (() => Element | null)} target
 * @property {string} title
 * @property {string} text
 */
/**
 * What `decorate` is told about the control it is handed: the same shape every kp module's `decorate` takes.
 * `kind` is `tour-next`, `tour-back` or `tour-skip`; `host` is the tour's card.
 * @typedef {{ kind: 'tour-next' | 'tour-back' | 'tour-skip', host: HTMLElement, key?: string, index?: number, label?: string, value?: string }} TourDecorateInfo
 */
/**
 * @typedef {object} TourOptions
 * @property {number} [start] the step to open on, counted from 0 among the steps that are on the page
 * @property {string} [remember] the name a finished or skipped tour is remembered under (js/remember.js, component `tour`)
 * @property {(part: HTMLElement, info: TourDecorateInfo) => void} [decorate] called with the card's buttons each time a tour builds them [R-DRIVE]
 * @property {HTMLElement | null} [returnFocus] where the focus goes when the tour ends; the focused element when it started, by default
 * @property {(finished: boolean) => void} [onEnd] called once when it ends: `true` when Done was pressed on the last step
 * @property {Partial<Strings>} [strings] any of the dictionary's `tour…` words, for this tour only
 */
/** What startTour hands back: end it, or go to a step (counted from 0 among the steps on the page). @typedef {{ end: () => void, goto: (i: number) => void }} TourHandle */
/** The distance from the card to the window's edges, and from the card to its target, in px. */
export declare const TOUR_GUTTER = 16;
export declare const TOUR_GAP = 12;
/**
 * The storage key a finished tour writes, as js/remember.js composes it:
 * `kp-remember:tour:<name>:done` with the default prefix.
 * @param {string} name
 * @returns {string}
 */
export declare const tourMemoryKey: (name: string) => string;
/**
 * Whether the tour of this name was taken before (finished or skipped).
 * @param {string} name
 * @param {{ storage?: Storage | null }} [options]
 * @returns {boolean}
 */
export declare const tourRemembered: (
  name: string,
  {
    storage,
  }?: {
    storage?: Storage | null;
  },
) => boolean;
/**
 * Forget that the tour of this name was taken, so it starts by itself again.
 * @param {string} name
 * @param {{ storage?: Storage | null }} [options]
 */
export declare const forgetTour: (
  name: string,
  {
    storage,
  }?: {
    storage?: Storage | null;
  },
) => void | undefined;
/**
 * Whether a tour starts by itself, and at which step: `?tour` (or `?tour=3`,
 * counted from 1) always does; otherwise only a first visit by a person
 * (not a browser driven by a script).
 * @param {{ search?: string, remembered?: boolean, automated?: boolean }} context
 * @returns {number | null} the step to start on, counted from 0, or null for no tour
 */
export declare function shouldStartTour({
  search,
  remembered,
  automated,
}: {
  search?: string;
  remembered?: boolean;
  automated?: boolean;
}): number | null;
/**
 * The steps a tour shows: those whose target `find` finds, in order. The
 * count on the card is taken from these, so it is exact.
 * @template {TourStep} S
 * @param {readonly S[]} steps
 * @param {(step: S) => unknown} find
 * @returns {S[]}
 */
export declare const tourStepsOnPage: <S extends TourStep>(
  steps: readonly S[],
  find: (step: S) => unknown,
) => S[];
/**
 * Where the card goes: centred on its target, kept `gutter` px inside the
 * window, and `gap` px under the target, or over it when the target's middle
 * is in the lower half of the window. Not rounded: a target at a fractional
 * position keeps the card exactly `gap` px from it.
 * @param {{ top: number, bottom: number, left: number, width: number, height: number }} target the target's box in the window
 * @param {{ width: number, height: number }} card
 * @param {{ width: number, height: number }} view
 * @param {{ gutter?: number, gap?: number }} [options]
 * @returns {{ left: number, top: number }}
 */
export declare function tourCardPlace(
  target: {
    top: number;
    bottom: number;
    left: number;
    width: number;
    height: number;
  },
  card: {
    width: number;
    height: number;
  },
  view: {
    width: number;
    height: number;
  },
  {
    gutter,
    gap,
  }?: {
    gutter?: number;
    gap?: number;
  },
): {
  left: number;
  top: number;
};
/**
 * Walk the reader over the page, one card per step, beside its target.
 * Steps whose target is not on the page are left out before counting, so the
 * count is exact. The card is a non-modal dialog: the page stays usable. Esc
 * or Skip ends it, the focus goes back where it was, and a tour that ended
 * is remembered (`remember`), if the browser lets it.
 * @param {TourStep[]} steps
 * @param {TourOptions} [options]
 * @returns {TourHandle | null} null when no step has a target on the page
 */
export declare function startTour(
  steps: TourStep[],
  { start, remember, decorate, returnFocus, onEnd, strings: own }?: TourOptions,
): TourHandle | null;
