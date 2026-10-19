// The kp-themes module chassis-rs serves at /static/kp/js/lede.js (kp-themes 11.0.0),
// copied from kp-themes js/lede.d.ts: chassis serves the module, not its types.
/** A lede. */
export declare const LEDE = ".kp-lede";
/**
 * The headings under `root` that have no lede after them.
 * @param {ParentNode} [root]
 * @returns {Element[]}
 */
export declare function missingLedes(root?: ParentNode): Element[];
/**
 * Wire every lede under `root`.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachLedes(root?: ParentNode): () => void;
