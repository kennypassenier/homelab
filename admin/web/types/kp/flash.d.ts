/** A button this module drives. */
export declare const FLASH_BUTTON = "[data-kp-flash-button], [data-kp-post]";
/** Fired on the button, bubbling, at every change of state: `{ state, ok?, word?, line? }`, state `busy` | `success` | `refused` | `rest`. */
export declare const FLASH_EVENT = "kp-flash";
/** Fired on the button when `data-kp-reload="region"` wants its region read again: `{ target }`. */
export declare const FLASH_RELOAD_EVENT = "kp-flash-reload";
/** How long a success word stays, in ms (chassis' value). */
export declare const FLASH_MS = 1500;
/** How long a refusal's word stays, in ms: a refusal needs longer to read. */
export declare const FLASH_ERROR_MS = 3000;
export type FlashAnswer =
  | string
  | void
  | {
      word?: string;
      line?: string;
    }
  | {
      error: string;
      remedy?: string;
      word?: string;
    };
export type FlashOptions = {
  flashMs?: number;
  errorMs?: number;
  busyLabel?: string;
  width?: "keep" | "grow";
  refusal?: "line" | "button" | "toast";
};
export type Wired = {
  stack: HTMLElement;
  rest: HTMLElement;
  busy: HTMLElement;
  result: HTMLElement;
  status: HTMLElement;
  line: HTMLElement;
  ownLine: boolean;
  timer: number;
  running: boolean;
  off: () => void;
};
/**
 * Run `work` behind a button: busy while it runs, then its result for a moment.
 *
 * @param {HTMLElement} button
 * @param {() => Promise<FlashAnswer> | FlashAnswer} work
 * @param {FlashOptions} [options]
 * @returns {Promise<{ ok: boolean, word: string, line: string } | null>} null when the press was ignored (already busy)
 */
export declare function runFlashButton(
  button: HTMLElement,
  work: () => Promise<FlashAnswer> | FlashAnswer,
  options?: FlashOptions,
): Promise<{
  ok: boolean;
  word: string;
  line: string;
} | null>;
/**
 * Wire every flash button under `root`.
 *
 * @param {ParentNode} [root]
 * @param {FlashOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachFlashButtons(
  root?: ParentNode,
  options?: FlashOptions,
): () => void;
