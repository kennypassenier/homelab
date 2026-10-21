/** A list toolbar. */
export declare const TOOLBAR = "[data-kp-toolbar]";
/** Fired on the toolbar when a tag's cross is pressed; `{ id, label }`. */
export declare const TOOLBAR_REMOVE_EVENT = "kp-toolbar-remove";
/** Fired on the toolbar when Clear all is pressed. */
export declare const TOOLBAR_CLEAR_EVENT = "kp-toolbar-clear";
export type ToolbarFilter = {
  id?: string;
  label: string;
};
/** @param {Element} toolbar @returns {HTMLInputElement | null} */
export declare function toolbarSearch(
  toolbar: Element,
): HTMLInputElement | null;
export type ToolbarDecorateInfo = {
  kind: "tag-remove" | "clear-all";
  host: HTMLElement;
  key?: string;
  value?: string;
  label?: string;
};
/**
 * What `decorate` is told about the control it is handed: `kind` is `tag-remove` (a tag's cross; `value` is the filter's id) or
 * `clear-all`; `host` is the toolbar and `key` its `data-kp-key`.
 * @typedef {{ kind: 'tag-remove' | 'clear-all', host: HTMLElement, key?: string, value?: string, label?: string }} ToolbarDecorateInfo
 */
/**
 * Draw the filters that are on under the toolbar: one removable tag each and
 * Clear all, or nothing (the row is hidden) when there are none. The Clear all
 * button and the word before the tags are made once and kept, so a press that
 * is still in flight never meets a button replaced under it.
 * @param {Element} toolbar
 * @param {readonly ToolbarFilter[]} filters
 * @param {{ clearAll?: boolean, removeGlyph?: string, decorate?: (part: HTMLElement, info: ToolbarDecorateInfo) => void }} [options]
 *   clearAll: show Clear all (default true); decorate: called with every tag's cross (`tag-remove`) and with Clear all (`clear-all`),
 *   each time they are built, the same shape every kp module's `decorate` takes [wish-1120, homelab 66, 114]
 */
export declare function setToolbarActive(
  toolbar: Element,
  filters: readonly ToolbarFilter[],
  {
    clearAll,
    removeGlyph,
    decorate,
  }?: {
    clearAll?: boolean;
    removeGlyph?: string;
    decorate?: (part: HTMLElement, info: ToolbarDecorateInfo) => void;
  },
): void;
/**
 * Wire every `[data-kp-toolbar]` under `root`: Escape in the search, the
 * search key, and the active row's two buttons. The active row is hidden when
 * it holds no tag.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachToolbars(root?: ParentNode): () => void;
