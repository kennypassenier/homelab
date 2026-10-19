// The kp-themes module chassis-rs serves at /static/kp/js/quantity.js (kp-themes 11.0.0),
// copied from kp-themes js/quantity.d.ts: chassis serves the module, not its types.
/** The controls this module looks after. */
export declare const QUANTITY = "[data-kp-quantity]";
/** Fired on the root when a person changed the value: `{ value, previous, source }`. */
export declare const QUANTITY_EVENT = "kp-quantity-change";
/** The first repeat of a held button, then the pace it settles to [ms]. */
export declare const HOLD_DELAY = 450;
export declare const HOLD_SLOW = 160;
export declare const HOLD_FAST = 70;
export type QuantityRules = {
  min: number;
  max: number;
  step: number;
  decimals: number;
  locale: string;
};
/**
 * The decimals of a step: 0.5 has one, 50 has none.
 * @param {number} step
 */
export declare function decimalsOf(step: number): number;
/**
 * The value written in the page's language: no grouping, the step's decimals.
 * @param {number} value @param {QuantityRules} rules
 */
export declare function formatQuantity(
  value: number,
  rules: QuantityRules,
): string;
/**
 * Read a typed number: a comma or a point is the decimal mark; with both, the
 * last one is, and the other groups thousands. Spaces are ignored. NaN when it
 * is not a number.
 * @param {string} text
 * @returns {number}
 */
export declare function parseQuantity(text: string): number;
/**
 * Keep a value inside the bounds and on a step (counted from the minimum when
 * there is one, else from 0), rounded to the step's decimals.
 * @param {number} value @param {QuantityRules} rules
 */
export declare function settleQuantity(
  value: number,
  rules: QuantityRules,
): number;
/** The value a root stands at, as a number (its last accepted value). @param {Element} root */
export declare function getQuantity(root: Element): number;
/**
 * Set the value without firing the event (a page that moves it because
 * something else moved), kept inside the bounds and on a step.
 * @param {Element} root @param {number} value
 */
export declare function setQuantity(root: Element, value: number): void;
/**
 * Wire every quantity under `root`: its parts, its buttons (a held button
 * repeats), its keys and its typing.
 * @param {ParentNode} [scope]
 * @returns {() => void} detach
 */
export declare function attachQuantities(scope?: ParentNode): () => void;
