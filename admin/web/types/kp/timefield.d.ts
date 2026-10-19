// The kp-themes module chassis-rs serves at /static/kp/js/timefield.js (kp-themes 11.0.0),
// copied from kp-themes js/timefield.d.ts: chassis serves the module, not its types.
/** The fields this module looks after. */
export declare const TIME = "[data-kp-time]";
/** Fired when the value changes: `{ value, text, converted }`. */
export declare const TIME_EVENT = "kp-time-change";
/** Fired by the Retry of presets that could not be loaded. */
export declare const TIME_RETRY_EVENT = "kp-time-retry";
/** The steps the keys and the list may use, in minutes. */
export declare const TIME_STEPS: readonly number[];
export type TimePreset = {
  label: string;
  value: string;
};
export type TimeState = {
  input: HTMLInputElement;
  hidden: HTMLInputElement;
  popup: HTMLElement;
  value: string;
  presets: TimePreset[] | "loading" | "error";
  open: boolean;
  active: number;
  lower: (() => void) | null;
  off: (() => void)[];
  invalid: string;
  converted: boolean;
};
/**
 * Read what a person typed or pasted as a time of day.
 * @param {string} text
 * @param {{ seconds?: boolean }} [options]
 * @returns {{ value: string, converted: boolean } | null} the time as `HH:MM`, and whether it was written in 12 hours; null when it is not a time
 */
export declare function parseTimeOfDay(
  text: string,
  {
    seconds,
  }?: {
    seconds?: boolean;
  },
): {
  value: string;
  converted: boolean;
} | null;
/** @param {string} value `HH:MM` or `HH:MM:SS` @returns {number} minutes since midnight */
export declare function timeMinutes(value: string): number;
/** @param {number} minutes @param {boolean} [seconds] */
export declare function timeText(minutes: number, seconds?: boolean): string;
/**
 * Step a time: up or down by `step` minutes, wrapping past midnight.
 * @param {string} value `HH:MM` or empty @param {number} delta steps (negative goes back) @param {number} step minutes
 * @returns {{ value: string, wrapped: boolean }}
 */
export declare function stepTime(
  value: string,
  delta: number,
  step: number,
): {
  value: string;
  wrapped: boolean;
};
/** Whether a time is inside the window; a min later than the max wraps midnight. @param {string} value @param {string | null} min @param {string | null} max */
export declare function timeInWindow(
  value: string,
  min: string | null,
  max: string | null,
): boolean;
/**
 * Set the value (no event unless `fire`), or clear it with an empty text.
 * @param {HTMLElement} root @param {string} value `HH:MM`, or '' @param {{ fire?: boolean }} [options]
 */
export declare function setTime(
  root: HTMLElement,
  value: string,
  {
    fire,
  }?: {
    fire?: boolean;
  },
): void;
/** The value as it stands (`HH:MM`, or '' while empty or invalid). @param {HTMLElement} root */
export declare function timeValue(root: HTMLElement): string;
/** Whether the field is fine: valid, and not empty when required. @param {HTMLElement} root */
export declare function timeValid(root: HTMLElement): boolean;
/**
 * Give the list its presets, or say they are loading or could not be loaded.
 * @param {HTMLElement} root @param {TimePreset[] | 'loading' | 'error'} presets
 */
export declare function setTimePresets(
  root: HTMLElement,
  presets: TimePreset[] | "loading" | "error",
): void;
/**
 * Wire every time field under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTimes(root?: ParentNode): () => void;
