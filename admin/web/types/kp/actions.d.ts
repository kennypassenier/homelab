// The kp-themes module chassis-rs serves at /static/kp/js/actions.js (kp-themes 10.0.0),
// copied from kp-themes js/actions.d.ts: chassis serves the module, not its types.
/** A row's button box. */
export declare const ROW_ACTIONS = ".kp-row-actions";
/** A list or table whose rows' buttons share columns. */
export declare const ACTION_LIST = ".kp-action-list, table";
/**
 * The roles of one row's buttons: the button's `data-kp-action` when it has
 * one, else its place from the row's end (`end-0` is the last); a role met
 * twice in one row is numbered.
 * @param {{ role?: string | null }[]} buttons in row order
 * @returns {string[]}
 */
export declare function rowRoles(
  buttons: {
    role?: string | null;
  }[],
): string[];
/**
 * One column order for every row: each row's roles keep their order, and a
 * role first met in a later row goes right after the role before it.
 * @param {string[][]} rows
 * @returns {string[]}
 */
export declare function mergeRoles(rows: string[][]): string[];
/**
 * Lay one list's or table's button boxes on shared columns.
 *
 * A list (`.kp-action-list`) is a CSS subgrid, so the browser sizes the
 * columns; this only names each button's column and the list's count, and
 * only when a button carries a role (by position the stylesheet needs no
 * help). A table cannot be a subgrid: there each role's widest button is
 * measured and the widths are written on the table.
 * @param {Element} list
 */
export declare function fitActionColumns(list: Element): void;
/**
 * Keep every list's and table's row buttons on shared columns under `root`:
 * fitted now, and again, once per frame, when a row's buttons change, the
 * theme changes or a font arrives (a table's widths are measured).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachActionColumns(root?: ParentNode): () => void;
