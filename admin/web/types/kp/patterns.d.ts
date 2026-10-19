/** A secret field [kpadopt4]: `.kp-secret[data-kp-secret]`. */
export declare const SECRET = "[data-kp-secret]";
/** How long a shown secret stays before it hides again, in seconds. Per element as `data-kp-secret-ttl` (0: until hidden). */
export declare const SECRET_TTL = 30;
/** The mask's length, in characters. Per element as `data-kp-secret-mask`. */
export declare const SECRET_MASK = 12;
/** Fired on the field, bubbling, when a secret is shown or hidden: `{ shown, reason }`, reason `press` | `hold` | `timer` | `away` | `call`. */
export declare const SECRET_EVENT = "kp-secret";
/** Fired on the field, bubbling and cancelable, when it needs the value: `{ respond(value), fail(reason), purpose }`; call preventDefault() to answer. */
export declare const SECRET_REQUEST_EVENT = "kp-secret-request";
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
 * `secretLoad(field, purpose)` reads a secret field's value when it is shown or copied (purpose `reveal` or `copy`), so the
 * value is in the page only while it is shown; without it a field asks through `kp-secret-request`, and last reads its
 * own `data-kp-secret-value`.
 *
 * @param {ParentNode} root
 * @param {{ copiedMs?: number, undoMs?: number, toastOnCopyFailure?: boolean, undoClassName?: string, secretLoad?: SecretLoad }} [options]
 * @returns {() => void} detach
 */
export declare function attachPatterns(
  root?: ParentNode,
  {
    copiedMs,
    undoMs,
    toastOnCopyFailure,
    undoClassName,
    secretLoad,
  }?: {
    copiedMs?: number;
    undoMs?: number;
    toastOnCopyFailure?: boolean;
    undoClassName?: string;
    secretLoad?: SecretLoad;
  },
): () => void;
export type SecretLoad = (
  field: HTMLElement,
  purpose: "reveal" | "copy",
) => string | Promise<string>;
export type SecretState = "hidden" | "loading" | "shown" | "error" | "disabled";
/**
 * Show a secret field's value, as Reveal does; no-op on a disabled field.
 * @param {Element} field
 */
export declare function revealSecret(field: Element): void;
/**
 * Hide a secret field's value again and take it out of the page.
 * @param {Element} field
 */
export declare function hideSecret(field: Element): void;
/**
 * Attach every secret field under `root`.
 * @param {ParentNode} [root]
 * @param {{ load?: SecretLoad, copiedMs?: number }} [options]
 * @returns {() => void} detach
 */
export declare function attachSecrets(
  root?: ParentNode,
  {
    load,
    copiedMs,
  }?: {
    load?: SecretLoad;
    copiedMs?: number;
  },
): () => void;
