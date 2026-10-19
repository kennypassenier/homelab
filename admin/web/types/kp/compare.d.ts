/** A compare slider. */
export declare const COMPARE = "[data-kp-compare]";
/** Fired on the slider when the divider moved by a person: `{ position }` (0 to 100). */
export declare const COMPARE_EVENT = "kp-compare-change";
/** Fired on the slider when the Retry of a failed image is pressed: `{ pane }` ('before' or 'after'). */
export declare const COMPARE_RETRY_EVENT = "kp-compare-retry";
/**
 * Hold a slider in its loading state (the busy card over the box) while the page reads its panes, or let it go.
 * @param {Element} el @param {boolean} busy
 */
export declare function setCompareBusy(el: Element, busy: boolean): void;
/**
 * Move a slider's divider from a script, without the event.
 * @param {Element} el @param {number} position 0 (all after) to 100 (all before)
 */
export declare function setCompare(el: Element, position: number): void;
/**
 * The divider's position after a key, or null for a key it does not take.
 * @param {string} key @param {number} at @param {boolean} big
 * @returns {number | null}
 */
export declare function compareKey(
  key: string,
  at: number,
  big: boolean,
): number | null;
/**
 * Wire every compare slider under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachCompares(root?: ParentNode): () => void;
