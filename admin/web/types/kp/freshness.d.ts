// The kp-themes module chassis-rs serves at /static/kp/js/freshness.js (kp-themes 10.0.0),
// copied from kp-themes js/freshness.d.ts: chassis serves the module, not its types.
/** A ticking line. */
export declare const AGO = "[data-kp-ago]";
/** The zone a moment is written in, whatever the reader's (rule 52). */
export declare const FRESHNESS_TIME_ZONE = "Europe/Brussels";
export type Strings = import("./strings.js").Strings;
/** @typedef {import('./strings.js').Strings} Strings */
/**
 * A span of seconds in the units a person reads best: seconds under a
 * minute, minutes and seconds under an hour, hours and minutes under a day,
 * days and hours beyond; a zero second part is left out ("2 min").
 * @param {number} seconds
 * @param {Strings} [strings]
 * @returns {string}
 */
export declare function humanDuration(
  seconds: number,
  strings?: Strings,
): string;
/**
 * The line's words: "updated 12 s ago", or "not updated yet" with no moment.
 * A moment after `nowMs` reads 0 s.
 * @param {string} verb
 * @param {number | null | undefined} atMs
 * @param {number} nowMs
 * @param {Strings} [strings]
 * @returns {string}
 */
export declare function agoText(
  verb: string,
  atMs: number | null | undefined,
  nowMs: number,
  strings?: Strings,
): string;
/**
 * A moment as a person reads it (rule 52): `dd/mm/yyyy HH:mm`, 24-hour, in
 * `timeZone`. The parts are read one by one, so no locale reorders or names
 * them.
 * @param {number} ms
 * @param {string} [timeZone]
 * @returns {string}
 */
export declare function agoMoment(ms: number, timeZone?: string): string;
/**
 * The moment an element carries, in ms, or null: its `datetime`, else the
 * value of `data-kp-ago` (ISO or milliseconds).
 * @param {Element} el
 * @returns {number | null}
 */
export declare function momentOf(el: Element): number | null;
export type AgoContext = {
  timeZone: string;
  now: () => number;
  /**
   * the attach's own words, over the dictionary's as they stand
   */
  overrides: Partial<Strings> | undefined;
  /**
   * the attach that found the line
   */
  owner: object;
};
export type AgoState = {
  /**
   * the moment last painted
   */
  at: number | null | undefined;
  /**
   * the text the reserved width was measured from
   */
  sample: string;
  /**
   * the stale state last painted (null before the first)
   */
  stale: boolean | null;
};
/**
 * Set a line's moment (ms since the epoch, or null for none) and repaint it
 * at once. The moment is written to `datetime` as ISO, for machines.
 * @param {HTMLElement} el
 * @param {number | null | undefined} ms
 */
export declare function setAgo(
  el: HTMLElement,
  ms: number | null | undefined,
): void;
/**
 * Keep every freshness line under `root` ticking, now and as lines arrive,
 * change or leave. One timer serves the whole page.
 *
 * @param {ParentNode} [root]
 * @param {{ timeZone?: string, now?: () => number, strings?: Partial<Strings> }} [options]
 * @returns {() => void} detach
 */
export declare function attachAgo(
  root?: ParentNode,
  {
    timeZone,
    now,
    strings,
  }?: {
    timeZone?: string;
    now?: () => number;
    strings?: Partial<Strings>;
  },
): () => void;
