// The kp-themes module chassis-rs serves at /static/kp/js/patterns.js (kp-themes 7.2.0),
// copied from kp-themes js/patterns.d.ts: chassis serves the module, not its types.
/** How long the undo stays offered before the action is committed. An operational knob; per element as `data-kp-undo-ms`. */
export declare const UNDO_MS = 6000;
/** How long "copied" stays on the button. Per element as `data-kp-copied-ms`. */
export declare const COPIED_MS = 1500;
export type ActionDetail = {
  target: HTMLElement;
  button: HTMLElement;
  key: string | null;
  restore: () => void;
};
/** @typedef {{ target: HTMLElement, button: HTMLElement, key: string | null, restore: () => void }} ActionDetail */
/** Fired on the row when an optimistic action commits — the undo window closed [TH26]. Detail: ActionDetail. */
export declare const COMMIT_EVENT = "kp-action-commit";
/** Fired on the row when it is undone instead. Detail: ActionDetail. */
export declare const UNDO_EVENT = "kp-action-undo";
/** Fired on the copy button after a successful copy: `{ text }`. */
export declare const COPY_EVENT = "kp-copy";
/** Fired on the copy button when the clipboard refused: `{ text, error }`. */
export declare const COPY_FAILED_EVENT = "kp-copy-failed";
/**
 * Attach the patterns under `root`.
 *
 * @param {ParentNode} root
 * @param {{ copiedMs?: number, undoMs?: number, toastOnCopyFailure?: boolean, undoClassName?: string }} [options]
 * @returns {() => void} detach
 */
export declare function attachPatterns(
  root?: ParentNode,
  {
    copiedMs,
    undoMs,
    toastOnCopyFailure,
    undoClassName,
  }?: {
    copiedMs?: number;
    undoMs?: number;
    toastOnCopyFailure?: boolean;
    undoClassName?: string;
  },
): () => void;
