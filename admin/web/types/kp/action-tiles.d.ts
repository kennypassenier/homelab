/** The tiles this module looks after. */
export declare const ACTION_TILE = ".kp-action";
/** Fired on the tile, bubbling, when it becomes busy or stops being busy: `{ tile, busy }`. */
export declare const ACTION_BUSY_EVENT = "kp-action-busy";
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
