/** A bar that folds. */
export declare const NAV_FOLD = "[data-kp-nav-fold]";
/** Fired on the nav, bubbling, when the set of folded items changed: `{ folded, items }`. */
export declare const NAV_FOLD_EVENT = "kp-nav-fold";
/** The item the script adds to the bar's list. */
export declare const NAV_MORE = "[data-kp-nav-more]";
/**
 * Which items do not fit: pure, so the arithmetic is read on its own.
 *
 * Items are kept from the start for as long as they fit beside the More
 * button; the first that does not fit folds with everything after it. An
 * item marked to keep stays in the bar wherever it is. When everything fits
 * without the More button, nothing folds.
 *
 * @param {object} at
 * @param {readonly number[]} at.widths each item's natural width
 * @param {number} at.gap the space between two items
 * @param {number} at.room the width the list has
 * @param {number} at.moreWidth the More item's width
 * @param {readonly boolean[]} [at.keep] items that never fold
 * @returns {boolean[]} per item: true when it folds
 */
export declare function planFold({
  widths,
  gap,
  room,
  moreWidth,
  keep,
}: {
  widths: readonly number[];
  gap: number;
  room: number;
  moreWidth: number;
  keep?: readonly boolean[];
}): boolean[];
/** A separator between two groups of links [kpadopt3]. */
export declare const NAV_SEP = ".kp-nav__sep";
/**
 * Which separators fold with the links around them: pure, beside planFold().
 *
 * Nothing folds while no link folds. Once one does, a separator stays only
 * where a shown link stands both before and after it in the bar, so no rule
 * is left at the start of the bar or against the More button.
 *
 * @param {readonly boolean[]} plan per item: true when it folds (planFold)
 * @param {readonly boolean[]} seps per item: true when it is a separator
 * @returns {boolean[]} the plan with the separators decided
 */
export declare function planSeparators(
  plan: readonly boolean[],
  seps: readonly boolean[],
): boolean[];
export type NavFoldOptions = {
  /**
   * any of the dictionary's `navMore…` words, for this bar only
   */
  strings?: Partial<import("./strings.js").Strings>;
};
/**
 * Fold one bar.
 *
 * @param {HTMLElement} nav the `.kp-nav` carrying `data-kp-nav-fold`
 * @param {NavFoldOptions} [options]
 * @returns {() => void} detach
 */
export declare function foldNav(
  nav: HTMLElement,
  { strings }?: NavFoldOptions,
): () => void;
/** The mark the React NavBar puts on a bar it wires itself [AR29]; it folds its own (`fold`). */
export declare const NAV_FOLD_OWNED = "[data-kp-nav-owner]";
/**
 * Fold every bar under `root` that carries `data-kp-nav-fold`. A bar the React
 * NavBar renders folds itself; pass `ownedBy: ''` to fold it here instead.
 *
 * @param {ParentNode} [root]
 * @param {NavFoldOptions & { ownedBy?: string }} [options]
 * @returns {() => void} detach
 */
export declare function attachNavFolds(
  root?: ParentNode,
  {
    ownedBy,
    ...options
  }?: NavFoldOptions & {
    ownedBy?: string;
  },
): () => void;
