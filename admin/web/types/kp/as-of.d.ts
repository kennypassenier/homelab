// The kp-themes module chassis-rs serves at /static/kp/js/as-of.js (kp-themes 10.0.0),
// copied from kp-themes js/as-of.d.ts: chassis serves the module, not its types.
/**
 * The elements under `root` now, to attach against later.
 * @param {ParentNode} root
 * @returns {WeakSet<Element>}
 */
export declare const presentUnder: (root: ParentNode) => WeakSet<Element>;
/**
 * Run `attach` against `root` as it stood when `present` was taken.
 *
 * @template T
 * @param {ParentNode} root
 * @param {WeakSet<Element>} present
 * @param {() => T} attach
 * @returns {T}
 */
export declare function asOf<T>(
  root: ParentNode,
  present: WeakSet<Element>,
  attach: () => T,
): T;
/**
 * The attribute a root wears while what attaches to it draws its first
 * render [fix-100]. js/auto.js puts it on the root of every attachAll()
 * (the `<html>` element for the document) and takes it off two painted
 * frames after the last module it fetched has attached; a consumer whose
 * framework renders a part of the page in several passes may wear it on
 * that part for as long. Nothing drawn under it is news: js/motion.js lets
 * no element arrive and no box glide there, it takes its first size at
 * once. Only what is added after the page settled (a new row, a new toast,
 * an opened group) arrives the theme's way.
 */
export declare const SETTLING_ATTRIBUTE = "data-kp-settling";
/**
 * Call `fn` once two frames have been painted: one frame lays out what was
 * drawn before it, the next is the first a reader sees settled, after the
 * resize observers of the first have run.
 * @param {() => void} fn
 */
export declare function afterPaint(fn: () => void): void;
/**
 * Mark `root` settling until `done` settles and two frames have been
 * painted after it; two holds on one root keep it settling until both let
 * go. The returned function lets go at once (a detach before the end).
 * @param {Element} root
 * @param {Promise<unknown>} done
 * @returns {() => void}
 */
export declare function settleAfter(
  root: Element,
  done: Promise<unknown>,
): () => void;
/**
 * Whether `el` is part of a first render still being drawn: under a root
 * that wears SETTLING_ATTRIBUTE.
 * @param {Element} el
 */
export declare const settling: (el: Element) => boolean;
