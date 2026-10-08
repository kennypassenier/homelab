// The kp-themes module chassis-rs serves at /static/kp/js/progressbar.js (kp-themes 10.0.0),
// copied from kp-themes js/progressbar.d.ts: chassis serves the module, not its types.
/** The bars this module looks after. */
export declare const PROGRESSBAR_SELECTOR = ".kp-progressbar";
/** The custom property the bar paints from, 0 to 1. */
export declare const VALUE_PROPERTY = "--kp-value";
/** The attribute that says "busy, no idea how far". */
export declare const INDETERMINATE_ATTRIBUTE = "data-kp-indeterminate";
/**
 * Write the track, the fill and the head into a bar that has no track yet.
 * A bar that has one is left alone, so markup a framework owns is never
 * written twice.
 *
 * @param {HTMLElement} el
 * @returns {HTMLElement} the bar
 */
export declare function buildProgressbar(el: HTMLElement): HTMLElement;
/**
 * Bring `--kp-value` in step with the bar's ARIA. A bar with no
 * `aria-valuenow` keeps whatever `--kp-value` it was given, so a consumer
 * who writes only the custom property is not overruled.
 *
 * @param {HTMLElement} el
 * @returns {number | null} the share written, or null when there was none to read
 */
export declare function syncProgressbar(el: HTMLElement): number | null;
/**
 * Set how far the bar is, in the bar's own range (`aria-valuemin` to
 * `aria-valuemax`, 0 to 100 when they are absent). It leaves the busy state.
 *
 * @param {HTMLElement} el
 * @param {number} value
 */
export declare function setProgress(el: HTMLElement, value: number): void;
/**
 * Turn "busy, no idea how far" on or off. On removes `aria-valuenow`, which
 * is how a screen reader learns the amount is unknown; off leaves the bar
 * at the value it is given next (or at `--kp-value`, until then).
 *
 * @param {HTMLElement} el
 * @param {boolean} on
 */
export declare function setIndeterminate(el: HTMLElement, on: boolean): void;
/**
 * Build and sync every `.kp-progressbar` under `root`, and keep each in step
 * when its `aria-valuenow`, `aria-valuemin` or `aria-valuemax` changes
 * later. A bar rendered after this ran is attached by calling it again on
 * that subtree.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach: stops following the attributes
 */
export declare function attachProgressbars(root?: ParentNode): () => void;
