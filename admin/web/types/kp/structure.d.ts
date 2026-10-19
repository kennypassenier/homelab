/** Fired on a tree when a branch opens or closes: `{ item, id, expanded }`. */
export declare const TREE_EXPAND_EVENT = "kp-tree-expand";
/** Fired on a tree when an item is selected: `{ item, id }`. */
export declare const TREE_SELECT_EVENT = "kp-tree-select";
/** Fired on a reorder list when the order changed. A contract value [TH26]: `{ order, id, from, to }`. */
export declare const REORDER_EVENT = "kp-reorder";
/** Fired on a split pane when the divider moved; the detail carries the percentage. */
export declare const SPLIT_EVENT = "kp-split";
/**
 * The tree's visible items, in the order a reader meets them.
 *
 * Collapsed branches are excluded, because Down should go to the next
 * thing you can see — walking into a closed folder is the bug that makes
 * a tree feel broken rather than merely awkward.
 *
 * @param {HTMLElement} tree
 */
export declare function visibleItems(tree: HTMLElement): HTMLElement[];
export type TreeHandle = {
  element: HTMLElement;
  expand: (id: string, expanded?: boolean) => void;
  expandAll: () => void;
  collapseAll: () => void;
  focusItem: (id: string) => void;
  select: (id: string | null) => void;
  selected: () => string | null;
};
export type ReorderHandle = {
  element: HTMLElement;
  order: () => string[];
  setOrder: (order: readonly string[]) => void;
};
export type SplitHandle = {
  element: HTMLElement;
  value: () => number;
  setValue: (value: number) => void;
};
/** The handle for an attached tree, reorder list or split pane. @param {Element} element */
export declare function structure(
  element: Element,
): ReorderHandle | SplitHandle | TreeHandle | null;
/**
 * @param {ParentNode} root
 * @param {{ typeahead?: boolean, typeaheadMs?: number, clickToggles?: boolean, selectable?: boolean, reorderPointer?: boolean, splitStep?: number, splitLargeStep?: number, collapseOnDoubleClick?: boolean }} [options]
 *   Defaults; per element as data-attributes: `data-kp-typeahead="false"`, `data-kp-click="select"`, `data-kp-selectable`, `data-kp-pointer="false"`, `data-kp-step`, `data-kp-large-step`, `data-kp-orientation="horizontal"`, `data-kp-collapse`.
 * @returns {(() => void) & { handles: (TreeHandle | ReorderHandle | SplitHandle)[] }} detach
 */
export declare function attachStructure(
  root?: ParentNode,
  {
    typeahead,
    typeaheadMs,
    clickToggles,
    selectable,
    reorderPointer,
    splitStep,
    splitLargeStep,
    collapseOnDoubleClick,
  }?: {
    typeahead?: boolean;
    typeaheadMs?: number;
    clickToggles?: boolean;
    selectable?: boolean;
    reorderPointer?: boolean;
    splitStep?: number;
    splitLargeStep?: number;
    collapseOnDoubleClick?: boolean;
  },
): (() => void) & {
  handles: (TreeHandle | ReorderHandle | SplitHandle)[];
};
