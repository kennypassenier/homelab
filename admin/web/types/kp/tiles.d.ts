// The kp-themes module chassis-rs serves at /static/kp/js/tiles.js (kp-themes 10.0.0),
// copied from kp-themes js/tiles.d.ts: chassis serves the module, not its types.
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
