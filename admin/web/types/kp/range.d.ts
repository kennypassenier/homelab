// The kp-themes module chassis-rs serves at /static/kp/js/range.js (kp-themes 11.1.1),
// copied from kp-themes js/range.d.ts: chassis serves the module, not its types.
/** A range picker. */
export declare const RANGE = "[data-kp-range]";
/** Fired on the picker when the window changes by a person: `{ preset, start, end, label }`. */
export declare const RANGE_EVENT = "kp-range-change";
/** The presets a picker offers when it does not say (`data-kp-range-presets`). */
export declare const RANGE_PRESETS: string[];
/**
 * A preset's length in ms ("24h", "7d", "90m", "2w"); 0 when unreadable.
 * @param {string} key
 */
export declare function presetMs(key: string): number;
/**
 * A moment as rule 52 writes it, in a zone: "16/10/2026 14:05".
 * @param {number} ms @param {string} [timeZone]
 */
export declare function rangeMoment(ms: number, timeZone?: string): string;
/**
 * A time typed as "14:05", "1405", "14.05" or "9"; null when it is not a time of day.
 * @param {string} text
 * @returns {{ hour: number, minute: number } | null}
 */
export declare function parseTime(text: string): {
  hour: number;
  minute: number;
} | null;
/**
 * What is wrong with a custom window, as dictionary keys; empty when it is right.
 * @param {number | null} start @param {number | null} end @param {{ now: number, future?: boolean, maxDays?: number }} rules
 * @returns {('order' | 'future' | 'span')[]}
 */
export declare function rangeFaults(
  start: number | null,
  end: number | null,
  {
    now,
    future,
    maxDays,
  }: {
    now: number;
    future?: boolean;
    maxDays?: number;
  },
): ("order" | "future" | "span")[];
export type RangeValue = {
  /**
   * the preset's key, or null for a custom window
   */
  preset: string | null;
  /**
   * ms
   */
  start: number;
  /**
   * ms
   */
  end: number;
};
/**
 * The window a picker holds, as ISO strings.
 * @param {Element} el
 * @returns {{ preset: string | null, start: string, end: string } | null}
 */
export declare function rangeOf(el: Element): {
  preset: string | null;
  start: string;
  end: string;
} | null;
/**
 * Set a picker's window from a script, without the event: a preset's key, or a start and an end (Date, ISO or ms).
 * @param {Element} el @param {{ preset?: string, start?: Date | string | number, end?: Date | string | number }} next
 */
export declare function setRange(
  el: Element,
  next: {
    preset?: string;
    start?: Date | string | number;
    end?: Date | string | number;
  },
): void;
/**
 * Wire every range picker under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number }} [options]
 * @returns {() => void} detach
 */
export declare function attachRanges(
  root?: ParentNode,
  {
    now,
  }?: {
    now?: () => number;
  },
): () => void;
