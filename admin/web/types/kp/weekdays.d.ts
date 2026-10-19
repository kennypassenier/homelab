// The kp-themes module chassis-rs serves at /static/kp/js/weekdays.js (kp-themes 11.0.0),
// copied from kp-themes js/weekdays.d.ts: chassis serves the module, not its types.
/** A weekday picker. */
export declare const WEEKDAYS = "[data-kp-weekdays]";
/** Fired on the picker when the days change by a person: `{ values, summary, valid }`. */
export declare const WEEKDAYS_EVENT = "kp-weekdays-change";
/** The quick picks a week picker offers when it does not say. */
export declare const WEEKDAY_PRESETS: readonly ["weekdays", "weekend", "every"];
/**
 * A day as a number 0 to 6 (0 is Sunday): a number, a digit string, or a name ("mon", "Tuesday").
 * @param {unknown} value
 * @returns {number | null}
 */
export declare function dayNumber(value: unknown): number | null;
/**
 * The weekday names of a style, Sunday first (the way `Date#getDay` counts). English reads the dictionary,
 * which a consumer may replace; another language reads Intl, unless the consumer replaced the dictionary.
 * @param {'short' | 'narrow' | 'long'} style
 * @param {string} [locale]
 * @returns {string[]}
 */
export declare function dayNames(
  style: "short" | "narrow" | "long",
  locale?: string,
): string[];
/**
 * "Mon and Thu", "Mon, Wed and Fri": a list in words.
 * @param {readonly string[]} items
 * @param {string} [and]
 */
export declare function joinWords(
  items: readonly string[],
  and?: string,
): string;
/**
 * The picked days in words, in the order a week reads from `firstDay`. Pure.
 * @param {readonly number[]} days 0 to 6
 * @param {{ firstDay?: number, names?: readonly string[] }} [options] `names` are Sunday first
 * @returns {string}
 */
export declare function weekdaySummary(
  days: readonly number[],
  {
    firstDay,
    names,
  }?: {
    firstDay?: number;
    names?: readonly string[];
  },
): string;
/**
 * The picked days of a month in words: "On the 1st and 15th", "On the last day".
 * @param {readonly (number | 'last')[]} days
 * @returns {string}
 */
export declare function monthSummary(
  days: readonly (number | "last")[],
): string;
export type WeekdaysHandle = {
  element: HTMLElement;
  /**
   * the picked days, in day order
   */
  values: () => (number | "last")[];
  /**
   * replace the picked days (no event)
   */
  set: (values: readonly (number | string)[]) => void;
  /**
   * the picked days in words
   */
  summary: () => string;
  /**
   * whether at least `min` days are picked
   */
  valid: () => boolean;
};
/**
 * The handle of an attached picker.
 * @param {Element} element
 */
export declare function weekdays(element: Element): WeekdaysHandle | null;
/**
 * The days a picker holds.
 * @param {Element} element
 * @returns {(number | 'last')[]}
 */
export declare function weekdaysOf(element: Element): (number | "last")[];
/**
 * Set the days of a picker from a script, without the event.
 * @param {Element} element @param {readonly (number | string)[]} values
 */
export declare function setWeekdays(
  element: Element,
  values: readonly (number | string)[],
): void;
/**
 * Wire every weekday picker under `root`.
 * @param {ParentNode} [root]
 * @param {{ locale?: string, weekStartsOn?: number }} [options] defaults; per picker `data-kp-week-starts-on`
 * @returns {() => void} detach
 */
export declare function attachWeekdays(
  root?: ParentNode,
  {
    locale: localeOption,
    weekStartsOn: weekOption,
  }?: {
    locale?: string;
    weekStartsOn?: number;
  },
): () => void;
