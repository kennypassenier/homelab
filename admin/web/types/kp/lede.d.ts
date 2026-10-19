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
