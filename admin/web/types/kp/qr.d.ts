/** The codes this module looks after. */
export declare const QR = "[data-kp-qr]";
/** Fired when the link was copied: `{ value }`. */
export declare const QR_COPY_EVENT = "kp-qr-copy";
/** Fired when a hidden code is shown or hidden: `{ shown }`. */
export declare const QR_REVEAL_EVENT = "kp-qr-reveal";
/** Fired once when the link's time ran out. */
export declare const QR_EXPIRED_EVENT = "kp-qr-expired";
/** Fired by the Retry of a code that could not be made. */
export declare const QR_RETRY_EVENT = "kp-qr-retry";
/** The characters of a link shown before it is folded behind "Show all". */
export declare const QR_FOLD = 160;
/** The error correction levels, lowest first. */
export declare const QR_LEVELS: readonly string[];
/** The plate's colours: dark modules on a light plate, whatever the theme (see the header). */
export declare const QR_PLATE: Readonly<{
  ground: "#ffffff";
  ink: "#000000";
}>;
export type QrSpec = {
  value?: string;
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the failed plate's sentence
   */
  message?: string;
};
export type QrState = {
  open: boolean;
  spec: QrSpec;
  mode: string;
  timer: number;
  shown: boolean;
  said: string;
  off: (() => void)[];
  card: import("./busy-card.js").BusyCard | null;
  expired: boolean;
};
/**
 * The matrix for a text: rows of booleans, or the reason it cannot be made.
 * @param {string} text @param {'L' | 'M' | 'Q' | 'H'} [level]
 * @returns {{ size: number, rows: boolean[][], bytes: number } | { error: string }}
 */
export declare function qrMatrix(
  text: string,
  level?: "L" | "M" | "Q" | "H",
):
  | {
      size: number;
      rows: boolean[][];
      bytes: number;
    }
  | {
      error: string;
    };
/** The WCAG contrast of two colours. @param {string} a @param {string} b */
export declare function qrContrast(a: string, b: string): number;
/**
 * Give a code its link or its state.
 * @param {HTMLElement} root @param {QrSpec} spec
 */
export declare function setQr(root: HTMLElement, spec: QrSpec): void;
/** The code's SVG text, or null when none is drawn. @param {HTMLElement} root */
export declare function qrSvg(root: HTMLElement): string | null;
/**
 * Wire every QR code under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachQr(root?: ParentNode): () => void;
