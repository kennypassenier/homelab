/** Fired for each accepted file. A contract value [TH26]: the detail carries the File and its row. */
export declare const FILE_EVENT = "kp-upload-file";
/** Fired for a file the zone refused: `{ file, reason, item }`. */
export declare const REJECT_EVENT = "kp-upload-reject";
/** Fired on the row before it is removed, cancelable: `{ file, item }`. */
export declare const REMOVE_EVENT = "kp-upload-remove";
export type RejectReason =
  "too-large" | "too-many" | "total-too-large" | "wrong-type" | string;
export type Validator = (file: File, accepted: File[]) => RejectReason | null;
/** @typedef {'too-large' | 'too-many' | 'total-too-large' | 'wrong-type' | string} RejectReason */
/** @typedef {(file: File, accepted: File[]) => RejectReason | null} Validator */
/** Does a file satisfy an `accept` list? Same rules as the file picker's own. @param {File} file @param {string} accept */
export declare function acceptsFile(file: File, accept: string): boolean;
export type UploadHandle = {
  element: HTMLElement;
  files: () => File[];
  add: (files: File[] | FileList) => void;
  clear: () => void;
};
/** The handle for an attached upload. @param {Element} element */
export declare function upload(element: Element): UploadHandle | null;
/**
 * @param {ParentNode} root
 * @param {{ locale?: string, validate?: Validator, renderRow?: (file: File) => HTMLElement, drop?: boolean, removeLabel?: string, removeClassName?: string }} [options]
 *   Defaults; per upload as data-attributes: `data-kp-max-bytes`, `data-kp-max-files`, `data-kp-max-total`, `data-kp-drop="false"`, `data-kp-locale`, and the input's own `accept`.
 * @returns {(() => void) & { handles: UploadHandle[] }} detach
 */
export declare function attachUploads(
  root?: ParentNode,
  {
    locale: localeOption,
    validate,
    renderRow,
    drop,
    removeLabel,
    removeClassName,
  }?: {
    locale?: string;
    validate?: Validator;
    renderRow?: (file: File) => HTMLElement;
    drop?: boolean;
    removeLabel?: string;
    removeClassName?: string;
  },
): (() => void) & {
  handles: UploadHandle[];
};
/**
 * Set a file row's progress. Exported because the consumer owns the
 * request and therefore owns the numbers.
 *
 * @param {HTMLElement} item
 * @param {number} percent
 */
export declare function setProgress(item: HTMLElement, percent: number): void;
/** Mark a row done, with an optional message. @param {HTMLElement} item @param {string} [message] */
export declare function setDone(item: HTMLElement, message?: string): void;
/**
 * Mark a row failed, with the reason where a screen reader will read it.
 *
 * @param {HTMLElement} item
 * @param {string} message
 */
export declare function setError(item: HTMLElement, message: string): void;
