/** The root attribute that makes a page calm. */
export declare const CALM_ATTRIBUTE = "data-kp-calm";
/** What one press turns off. */
export declare const CALM_SCOPES: readonly ["motion", "effects", "all"];
/** The switch. */
export declare const CALM_SWITCH = "[data-kp-calm-switch]";
/** Fired on the root, bubbling, when calm changes: `{ calm, scope, loading }`. */
export declare const CALM_EVENT = "kp-calm-change";
export type CalmScope = "motion" | "effects" | "all";
/** Whether the system itself asks for less motion. @param {Document} [doc] */
export declare function systemReduced(doc?: Document): boolean;
/** Whether the page is calm by its own flag (not the system's). @param {Document | Element} [where] */
export declare function isCalm(where?: Document | Element): boolean;
/** @param {Document | Element} [where] @returns {CalmScope} */
export declare function calmScope(where?: Document | Element): CalmScope;
/**
 * May this element move? False when the system asks for less motion or the page is calm. Every script that draws
 * movement asks this instead of the media query, so the two switches are one.
 *
 * @param {Element | Document} [where]
 * @returns {boolean}
 */
export declare function motionAllowed(where?: Element | Document): boolean;
export type CalmOptions = {
  doc?: Document;
  /**
   * what one press turns off (default: the page's, else `effects`)
   */
  scope?: CalmScope;
  /**
   * how a loading part behaves (default: the page's, else `slow`; `all` implies `still`)
   */
  loading?: "slow" | "still";
  /**
   * keep the choice on this device (default true)
   */
  remember?: boolean;
  /**
   * say it once, politely (default true)
   */
  announce?: boolean;
};
/**
 * @typedef {object} CalmOptions
 * @property {Document} [doc]
 * @property {CalmScope} [scope] what one press turns off (default: the page's, else `effects`)
 * @property {'slow' | 'still'} [loading] how a loading part behaves (default: the page's, else `slow`; `all` implies `still`)
 * @property {boolean} [remember] keep the choice on this device (default true)
 * @property {boolean} [announce] say it once, politely (default true)
 */
/**
 * Turn calm mode on or off.
 *
 * @param {boolean} on
 * @param {CalmOptions} [options]
 */
export declare function setCalm(
  on: boolean,
  { doc, scope, loading, remember, announce }?: CalmOptions,
): void;
/**
 * Put the stored choice back onto the root, before the first frame (js/auto.js calls it with the stored theme).
 * @param {Document} [doc]
 */
export declare function applyStoredCalm(doc?: Document): void;
/**
 * Wire every calm switch under `root`: it shows the flag, sets it, and shows on and disabled while the system asks
 * for less motion.
 *
 * @param {ParentNode} [root]
 * @param {CalmOptions} [options] the scope and loading behaviour a press applies
 * @returns {() => void} detach
 */
export declare function attachCalm(
  root?: ParentNode,
  options?: CalmOptions,
): () => void;
