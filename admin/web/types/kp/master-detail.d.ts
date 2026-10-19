// The kp-themes module chassis-rs serves at /static/kp/js/master-detail.js (kp-themes 11.0.0),
// copied from kp-themes js/master-detail.d.ts: chassis serves the module, not its types.
/** A list-and-detail layout. */
export declare const MASTER_DETAIL = "[data-kp-master-detail]";
/** Fired when the selection moves. */
export declare const PANE_SELECT_EVENT = "kp-pane-select";
/** Fired when the detail is opened. */
export declare const PANE_OPEN_EVENT = "kp-pane-open";
/** Fired when the folded detail, or the aside, is closed. */
export declare const PANE_CLOSE_EVENT = "kp-pane-close";
/** Fired when Retry is pressed in an errored detail. */
export declare const PANE_RETRY_EVENT = "kp-pane-retry";
/** The default widths of the list and the detail, and of the aside. */
export declare const PANE_WIDTHS = "17rem 1fr";
export declare const PANE_ASIDE_WIDTH = "24rem";
/** The layout's width the panes fold below, and the one the aside folds below. */
export declare const PANE_FOLD_AT = "48rem";
export declare const PANE_ASIDE_FOLD_AT = "90rem";
export type PaneState = "empty" | "loading" | "error" | "ready";
export type PaneStateOptions = {
  /**
   * the plate's first line
   */
  title?: string;
  /**
   * its sentence
   */
  text?: string;
  /**
   * what the busy card says (default `paneLoading`)
   */
  words?: string;
  /**
   * an action under an empty plate
   */
  actionLabel?: string;
  onAction?: (event: MouseEvent) => void;
};
export type MasterDetailHandle = {
  element: HTMLElement;
  /**
   * the selected key
   */
  key: () => string | null;
  select: (
    key: string | null,
    how?: {
      focus?: boolean;
    },
  ) => void;
  /**
   * select and, folded, show the detail
   */
  open: (
    key: string,
    how?: {
      focus?: boolean;
    },
  ) => void;
  /**
   * show the list again (folded)
   */
  close: () => void;
  /**
   * whether the panes are folded now
   */
  narrow: () => boolean;
  setState: (state: PaneState, options?: PaneStateOptions) => void;
};
/** The handle of an attached layout. @param {Element} element */
export declare const masterDetail: (
  element: Element,
) => MasterDetailHandle | null;
/**
 * Set the detail pane's state. A page that draws with the DOM calls this; the
 * detail pane keeps its place and size in every state.
 * @param {Element} root
 * @param {PaneState} state
 * @param {PaneStateOptions} [options]
 */
export declare function setPaneState(
  root: Element,
  state: PaneState,
  options?: PaneStateOptions,
): void;
/**
 * Select an item by its key (the arrow keys' move), without opening the detail.
 * @param {Element} root @param {string | null} key
 */
export declare function selectPane(root: Element, key: string | null): void;
/**
 * Open the detail of an item (narrow: the detail replaces the list).
 * @param {Element} root @param {string} key
 */
export declare function openPane(root: Element, key: string): void;
/** Show the list again. @param {Element} root */
export declare function closePane(root: Element): void;
/**
 * Wire every master-detail layout under `root`.
 * @param {ParentNode} [root]
 * @param {{ history?: boolean }} [options] `history: false` leaves the browser's history alone
 * @returns {() => void} detach
 */
export declare function attachMasterDetails(
  root?: ParentNode,
  {
    history: useHistory,
  }?: {
    history?: boolean;
  },
): () => void;
