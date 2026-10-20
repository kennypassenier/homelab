// The kp-themes module chassis-rs serves at /static/kp/js/undo-toast.js (kp-themes 11.1.1),
// copied from kp-themes js/undo-toast.d.ts: chassis serves the module, not its types.
/** The toast's own class. */
export declare const UNDO_TOAST = "kp-toast--undo";
/** Fired on the toast region, bubbling, when the action is committed, undone or failed: `{ toast, outcome }`. */
export declare const UNDO_TOAST_EVENT = "kp-undo-toast";
/** How long the toast says "Restored" [ms]. */
export declare const UNDO_RESTORED_MS = 1500;
/** How many toasts stand at once. */
export declare const UNDO_MAX = 3;
export type UndoToastOptions = {
  /**
   * runs when the window closes (mode `defer`); a rejection is a failure
   */
  commit?: () => unknown | Promise<unknown>;
  /**
   * runs on Undo
   */
  undo?: () => unknown;
  /**
   * runs when a commit failed (default `undo`)
   */
  restore?: () => unknown;
  /**
   * the window; 0 commits at once
   */
  ms?: number;
  mode?: "defer" | "revert";
  clock?: "ring" | "bar" | "text" | "none";
  /**
   * `Mod+Z`
   */
  key?: string | false;
  leave?: "carry" | "commit" | "cancel";
  max?: number;
  /**
   * the toast's line when the commit fails, instead of "Not done: why"
   */
  failed?: string;
  /**
   * the toast's line after Undo, instead of "Restored"
   */
  restored?: string;
  region?: HTMLElement | null;
};
export type UndoToast = {
  toast: HTMLElement & {
    dismiss: () => void;
  };
  state: () => "counting" | "committing" | "failed" | "undone" | "done";
  /**
   * undo now
   */
  undo: () => void;
  /**
   * commit now
   */
  accept: () => Promise<void>;
  /**
   * settles with how it ended
   */
  done: Promise<"committed" | "undone" | "failed" | "left">;
};
/**
 * Show a toast with Undo and a clock.
 * @param {string} text what happened: "3 schedules deleted"
 * @param {UndoToastOptions} [options]
 * @returns {UndoToast}
 */
export declare function undoToast(
  text: string,
  options?: UndoToastOptions,
): UndoToast;
