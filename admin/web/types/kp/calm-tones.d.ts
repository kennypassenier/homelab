/** The attribute that makes one of them calm. */
export declare const CALM_TONES_ATTRIBUTE = "data-kp-tones";
/** The tones only a calm component draws, and the ones it shares. */
export declare const CALM_TONES: readonly [
  "logged",
  "quiet",
  "paused",
  "future",
  "before",
  "loading",
];
export type CalmTone =
  "logged" | "quiet" | "paused" | "future" | "before" | "loading";
/**
 * Is this component calm?
 * @param {Element | null | undefined} el
 */
export declare const isCalmTones: (el: Element | null | undefined) => boolean;
/**
 * A tone in the calm vocabulary. Pure apart from the one console warning per foreign tone.
 * @param {string | undefined} tone what the page handed over
 * @param {Element | null} [el] the calm component (its `data-kp-calm-map` decides)
 * @returns {string} a calm tone, or the tone itself when the page switched the mapping off
 */
export declare function calmTone(
  tone: string | undefined,
  el?: Element | null,
): string;
/**
 * The words of a calm tone, for a screen reader and a tooltip: "logged", "nothing logged", "paused", never "missed".
 * @param {string} tone
 * @returns {string | null} null for a tone this vocabulary does not name
 */
export declare function calmWord(tone: string): string | null;
/**
 * Whether `iso` (`YYYY-MM-DD`) lies in a pause given as `from/to` (both inclusive; `from/` has no end, `/to` no start).
 * @param {string | null | undefined} range
 * @param {string} iso
 */
export declare function inPausedRange(
  range: string | null | undefined,
  iso: string,
): boolean;
/**
 * The legend of a calm component: logged, nothing logged, paused, in the words of the dictionary (`calmLegend…`). A heat grid draws
 * it by itself (js/heat.js) unless `data-kp-legend="hide"`.
 * @returns {{ tone: 'logged' | 'quiet' | 'paused', label: string }[]}
 */
export declare function calmLegendEntries(): {
  tone: "logged" | "quiet" | "paused";
  label: string;
}[];
