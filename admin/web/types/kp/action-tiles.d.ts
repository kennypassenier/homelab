/** The tiles this module looks after. */
export declare const ACTION_TILE = ".kp-action";
/** Fired on the tile, bubbling, when it becomes busy or stops being busy: `{ tile, busy }`. */
export declare const ACTION_BUSY_EVENT = "kp-action-busy";
/** The tile sizes the stylesheet knows; no size is the theme's own. */
export declare const ACTION_SIZES: readonly ["compact", "large"];
/**
 * Set the size of the tiles under `element` (or of one tile): `compact`, `large`, or `null` for the theme's own.
 * The stylesheet follows the attribute, so this is only a typed way to write it.
 *
 * @param {Element} element a `.kp-actions` grid, a tile, or any element above them
 * @param {(typeof ACTION_SIZES)[number] | null} size
 * @returns {Element}
 */
export declare function setActionSize(
  element: Element,
  size: (typeof ACTION_SIZES)[number] | null,
): Element;
/**
 * Make a tile busy or not busy. `aria-busy` is the single source of truth;
 * the spinner and the spoken word follow it.
 *
 * @param {Element} tile
 * @param {boolean} [busy] true when absent
 * @returns {Element}
 */
export declare function setActionBusy(tile: Element, busy?: boolean): Element;
/**
 * Stop a press on a busy or disabled tile, and keep every tile's spinner in
 * step with `aria-busy`, under `root` and for tiles added to it later.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachActionTiles(root?: ParentNode): () => void;
