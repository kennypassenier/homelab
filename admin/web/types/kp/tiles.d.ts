/** The marker of a board whose grids share one tile height. */
export declare const TILES_SET = "[data-kp-tiles-set]";
/** The variable a set writes on each of its grids: the floor of a row. */
export declare const TILE_ROW_MIN = "--kp-tile-row-min";
/**
 * The sets under `root`, each with its grids: by name where the marker has
 * one, else by the marked element.
 * @param {ParentNode} root
 * @returns {Map<unknown, HTMLElement[]>}
 */
export declare function tileSets(root: ParentNode): Map<unknown, HTMLElement[]>;
/**
 * Even one set: every grid's rows at least as tall as the set's tallest
 * visible tile. Each tile is read at its natural height while the grids
 * carry `data-kp-tiles-measuring` (the stylesheet lets their rows size to
 * content for that one synchronous step), and the floor is written only
 * where it changed.
 * @param {HTMLElement[]} grids
 * @returns {number} the floor, in px (0 for a set with no visible tile)
 */
export declare function evenTileSet(grids: HTMLElement[]): number;
/**
 * Keep every tile set under `root` even: now, and whenever a tile changes
 * size or content, a grid's width changes, a fold opens or closes, or a set
 * or grid is added or removed.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTileSets(root?: ParentNode): () => void;
/** The marker of a board whose tiles are one place in the tab order, moved with the arrow keys. */
export declare const TILES_KEYS = "[data-kp-tiles-keys]";
/** Fired on the board when the arrow keys moved the focus to another tile: `{ from, to, key }`. */
export declare const TILES_MOVE_EVENT = "kp-tiles-move";
/** Fired on the board when Up was pressed on the first row: `{ direction: 'up' }`. */
export declare const TILES_EXIT_EVENT = "kp-tiles-exit";
/**
 * The element of a tile that takes the focus: the one marked `data-kp-tile-stop`, else the tile itself when it can be focused,
 * else its first link, button or field.
 * @param {Element} tile
 * @returns {HTMLElement | null}
 */
export declare function tileStop(tile: Element): HTMLElement | null;
/**
 * Where to go from a tile: pure geometry over the tiles' boxes. Left and Right keep to the row (the tiles whose centres stand
 * within half a tile of this one's), and wrap to the next or the previous row at its end; Up and Down take the nearest row in
 * that direction and, in it, the tile nearest in the column, so a group with a short last row sends Down to its last tile rather
 * than skipping a row.
 *
 * @param {readonly { left: number, right: number, top: number, bottom: number }[]} boxes the tiles, in document order
 * @param {number} from index of the tile the focus is on
 * @param {'left' | 'right' | 'up' | 'down' | 'first' | 'last'} way
 * @returns {number | null} the index to go to; null where there is nowhere (the first row's Up, the last row's Down)
 */
export declare function nextTile(
  boxes: readonly {
    left: number;
    right: number;
    top: number;
    bottom: number;
  }[],
  from: number,
  way: "left" | "right" | "up" | "down" | "first" | "last",
): number | null;
/**
 * Make the tiles under every `[data-kp-tiles-keys]` one place in the tab order, moved with the arrow keys. Idempotent.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTileKeys(root?: ParentNode): () => void;
