/** The shell. */
export declare const VIEWER = ".kp-viewer";
/** Fired on the document, bubbling, when a viewer closed: `{ dialog }`. */
export declare const VIEWER_CLOSE_EVENT = "kp-viewer-close";
/** The states a stage can be in. */
export declare const VIEWER_STATES: readonly [
  "ready",
  "loading",
  "error",
  "unsupported",
];
export type ViewerAction = {
  kind: "open" | "download" | "print" | "copy-link" | string;
  /**
   * the file's address (open, download, copy-link)
   */
  href?: string;
  /**
   * the file name a download is saved as
   */
  name?: string;
  /**
   * what a print or a custom action does
   */
  run?: () => void;
  /**
   * the button's words (default: the dictionary's for the kind)
   */
  label?: string;
  /**
   * a custom action's look
   */
  tone?: "destructive" | "default";
};
export type ViewerOptions = {
  title: string;
  /**
   * one line under the title: how it is shown
   */
  desc?: string;
  /**
   * kind, size and date in one line
   */
  facts?: string;
  /**
   * about 70 % of the window and 90 % high, or all of it (default wide)
   */
  size?: "wide" | "full";
  actions?: readonly ViewerAction[];
  /**
   * more classes on the dialog
   */
  className?: string;
  /**
   * where the focus goes back to when the native return finds nothing
   */
  opener?: Element | null;
  onClose?: () => void;
  /**
   * shortcuts that live while the viewer is open
   */
  keys?: readonly import("./shortcuts.js").Shortcut[];
  doc?: Document;
};
export type ViewerHandle = {
  dialog: HTMLDialogElement;
  /**
   * the box the content stands in
   */
  stage: HTMLElement;
  /**
   * the content slot inside the stage
   */
  content: HTMLElement;
  /**
   * the foot with the actions
   */
  foot: HTMLElement;
  setTitle: (text: string) => void;
  setDesc: (text: string) => void;
  setFacts: (text: string) => void;
  /**
   * "4 of 37", exact, announced politely
   */
  setPosition: (
    position: {
      index: number;
      total: number;
    } | null,
  ) => void;
  setActions: (actions: readonly ViewerAction[]) => void;
  setContent: (...nodes: Node[]) => void;
  setState: (
    state: "ready" | "loading" | "error" | "unsupported",
    detail?: {
      message?: string;
      retry?: () => void;
      title?: string;
    },
  ) => void;
  state: () => string;
  close: () => void;
};
/**
 * Open a viewer.
 * @param {ViewerOptions} options
 * @returns {ViewerHandle}
 */
export declare function openViewer(options: ViewerOptions): ViewerHandle;
