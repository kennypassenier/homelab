/** The controls this module looks after. */
export declare const QUIET = "[data-kp-quiet]";
/** Fired on the control when a person changes it: `{ value, summary, valid }`. */
export declare const QUIET_EVENT = "kp-quiet-change";
/** The severities, most serious first, when the page names none. */
export declare const QUIET_SEVERITIES: readonly [
  "critical",
  "warning",
  "info",
  "ok",
];
export type QuietValue = {
  /**
   * quiet hours are on
   */
  on: boolean;
  /**
   * `HH:MM`
   */
  from: string;
  /**
   * `HH:MM`
   */
  to: string;
  /**
   * 0 (Sunday) to 6
   */
  days: number[];
  /**
   * the severities that still come through
   */
  through: string[];
};
/**
 * A value read from whatever the page wrote, with the defaults for what is missing. Pure.
 * @param {unknown} raw an object, or its JSON text
 * @returns {QuietValue}
 */
export declare function readQuiet(raw: unknown): QuietValue;
/**
 * The value in words: "Quiet from 22:00 to 07:00: Every weekday. Critical still comes through." Pure.
 * @param {QuietValue} value
 * @param {{ severities?: readonly { key: string, label: string }[], firstDay?: number }} [options]
 * @returns {string}
 */
export declare function quietSummary(
  value: QuietValue,
  {
    severities,
    firstDay,
  }?: {
    severities?: readonly {
      key: string;
      label: string;
    }[];
    firstDay?: number;
  },
): string;
/**
 * The value a control holds.
 * @param {Element} el
 * @returns {QuietValue | null}
 */
export declare function quietValue(el: Element): QuietValue | null;
/**
 * Set the control from the page's stored value (no event).
 * @param {Element} el an attached `[data-kp-quiet]`
 * @param {Partial<QuietValue>} value
 */
export declare function setQuiet(el: Element, value: Partial<QuietValue>): void;
/**
 * Build every quiet-hours control under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachQuietHours(root?: ParentNode): () => void;
