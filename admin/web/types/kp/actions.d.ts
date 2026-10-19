// The kp-themes module chassis-rs serves at /static/kp/js/actions.js (kp-themes 11.0.0),
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
/** A list whose rows the keyboard walks. */
export declare const ROW_KEYS = "[data-kp-row-keys]";
/** Fired on a row, bubbling, when Enter is pressed on the row itself: `{ row }`. */
export declare const ROW_OPEN_EVENT = "kp-row-open";
/**
 * Make a row busy while its action runs, or done with it. The row carries
 * `aria-busy`; every button in it says no to a press (`aria-disabled`, so the
 * focus stays where it is); `button`, the one pressed, shows the theme's
 * spinner in place of its words, which keep their room, so no column moves.
 * @param {HTMLElement} row a list item of a `.kp-action-list`
 * @param {boolean} [busy] true when absent
 * @param {HTMLElement | null} [button] the button that started it
 * @returns {HTMLElement} the row
 */
export declare function setRowBusy(
  row: HTMLElement,
  busy?: boolean,
  button?: HTMLElement | null,
): HTMLElement;
/**
 * Let a row leave the theme's way and take it out (js/motion.js leave(); at
 * once under reduced motion). When the focus is in it, the focus goes first
 * to the same place in the next row (the button at the same position, or the
 * row itself), else the row before, else the list.
 * @param {HTMLElement} row
 * @returns {Promise<void>} settled once it is gone
 */
export declare function removeRow(row: HTMLElement): Promise<void>;
/**
 * Let the keyboard walk the rows of every list marked `data-kp-row-keys`
 * under `root`: j or ↓ to the next row, k or ↑ to the one before, Home and
 * End to the first and the last, Enter on a row fires `kp-row-open`. Never
 * while typing in a field or with a modifier held, and a button in a row
 * keeps Enter for itself. One row of the list is in the tab order (the
 * last one focused), so Tab reaches the list once.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachRowKeys(root?: ParentNode): () => void;
