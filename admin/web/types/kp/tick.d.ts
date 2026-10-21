/** An element whose number ticks when its text changes. */
export declare const TICK = "[data-kp-tick]";
/** How long a tick takes, in ms. */
export declare const TICK_KNOB = "--kp-tick";
/** The default length of a tick, in ms. */
export declare const TICK_MS = 600;
/** Fired when a tick lands: `{ value }`. */
export declare const TICK_EVENT = "kp-tick";
export type TickParse = {
  value: number;
  decimals: number;
  /**
   * the text with another value in the same shape
   */
  render: (value: number) => string;
};
/**
 * @typedef {object} TickParse
 * @property {number} value
 * @property {number} decimals
 * @property {(value: number) => string} render  the text with another value in the same shape
 */
/**
 * Read the first number out of `text` in the shape `locale` writes numbers,
 * with a way to write another value back in the same shape (the same text
 * around it, grouping and decimals). Null when the text holds no number.
 * @param {string} text @param {string | undefined} locale
 * @returns {TickParse | null}
 */
export declare function parseTick(
  text: string,
  locale: string | undefined,
): TickParse | null;
/**
 * Count `el` from the number it shows to the number in `text`, and end on
 * `text` exactly. Without a number in either, `text` is written at once.
 * @param {Element} el
 * @param {string} [text]  the new text; left out, the element's own text is the target (it was just written)
 * @param {{ from?: number, duration?: number }} [options]  `from` the number to start at (default the one last shown)
 */
export declare function tickTo(
  el: Element,
  text?: string,
  {
    from,
    duration,
  }?: {
    from?: number;
    duration?: number;
  },
): void;
/**
 * Watch every `[data-kp-tick]` under `root` (and those added later): when
 * its text is changed by anyone but this module, it ticks from the number it
 * showed to the new one.
 * @param {ParentNode} [root]
 * @returns {() => void} detach: every running tick lands on its number
 */
export declare function attachTicks(root?: ParentNode): () => void;
