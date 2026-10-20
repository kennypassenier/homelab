// The kp-themes module chassis-rs serves at /static/kp/js/unsaved.js (kp-themes 11.1.1),
// copied from kp-themes js/unsaved.d.ts: chassis serves the module, not its types.
/** An unsaved-changes bar. */
export declare const UNSAVED = "[data-kp-unsaved]";
/** Fired on the bar when the count changes: `{ count, changes }`. */
export declare const UNSAVED_CHANGE_EVENT = "kp-unsaved-change";
/** Fired on the bar, bubbling and cancelable, when Save is pressed: `{ changes }`. */
export declare const UNSAVED_SAVE_EVENT = "kp-unsaved-save";
/** Fired on the bar when the changes were discarded: `{ changes }`; and `kp-unsaved-undo` when they were put back. */
export declare const UNSAVED_DISCARD_EVENT = "kp-unsaved-discard";
/** Fired on the bar when a discard is undone: `{ changes }`. */
export declare const UNSAVED_UNDO_EVENT = "kp-unsaved-undo";
/** Fired on the bar, bubbling and cancelable, when a link would leave a page with unsaved changes: `{ href }`. */
export declare const UNSAVED_LEAVE_EVENT = "kp-unsaved-leave";
/** Fired on the bar, bubbling and cancelable, when one change is reverted: `{ key, row }`. */
export declare const UNSAVED_REVERT_EVENT = "kp-unsaved-revert";
export type UnsavedChange = {
  /**
   * the row's name
   */
  key: string;
  /**
   * what it was
   */
  was: string;
  /**
   * what it is
   */
  now: string;
  /**
   * the row (or the control) that carries `data-kp-changed`
   */
  row: HTMLElement;
};
export type UnsavedHandle = {
  element: HTMLElement;
  /**
   * the changes now
   */
  changes: () => UnsavedChange[];
  state: () => "clean" | "dirty" | "saving";
  /**
   * put every value back (with Undo)
   */
  discard: () => void;
  /**
   * the page's answer to Save
   */
  saved: (result?: { refused?: Record<string, string> }) => void;
  /**
   * take what the form holds now as the saved values
   */
  markClean: () => void;
  setSaving: (saving: boolean) => void;
};
/** The handle of an attached bar. @param {Element} element */
export declare function unsaved(element: Element): UnsavedHandle | null;
/**
 * The page's answer to Save: the changes that went through stop counting, those in `refused` (their key and the
 * reason) stay marked and the bar says why.
 * @param {Element} element @param {{ refused?: Record<string, string> }} [result]
 */
export declare function unsavedSaved(
  element: Element,
  result?: {
    refused?: Record<string, string>;
  },
): void;
/**
 * Take what the form holds now as the saved values: nothing counts as changed.
 * @param {Element} element
 */
export declare function markUnsavedClean(element: Element): void;
/**
 * Put every changed value back, offering Undo, as Discard does.
 * @param {Element} element
 */
export declare function discardUnsaved(element: Element): void;
/**
 * Put the bar at work (Save shows the spinner) or at rest, without the page's answer.
 * @param {Element} element @param {boolean} saving
 */
export declare function setUnsavedSaving(
  element: Element,
  saving: boolean,
): void;
/**
 * Wire every unsaved-changes bar under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachUnsaved(root?: ParentNode): () => void;
