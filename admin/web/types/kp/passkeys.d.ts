// The kp-themes module chassis-rs serves at /static/kp/js/passkeys.js (kp-themes 11.0.0),
// copied from kp-themes js/passkeys.d.ts: chassis serves the module, not its types.
/** A passkey list the page fills with setPasskeys(). */
export declare const PASSKEYS = "[data-kp-passkeys]";
/** Fired on the root when Register (the empty list's button) is pressed. */
export declare const PASSKEYS_REGISTER_EVENT = "kp-passkeys-register";
/** Fired on the root when the Retry of a failed list is pressed. */
export declare const PASSKEYS_RETRY_EVENT = "kp-passkeys-retry";
/** Fired on the root, cancelable, when a removal was confirmed and the page gave no `remove`: `{ key, row }`. */
export declare const PASSKEYS_REMOVE_EVENT = "kp-passkeys-remove";
/** Fired on the root, cancelable, when a new name was saved and the page gave no `rename`: `{ key, row, label }`. */
export declare const PASSKEYS_RENAME_EVENT = "kp-passkeys-rename";
/** The kinds of way in, each with its glyph. */
export declare const PASSKEY_KINDS: readonly string[];
export type PasskeyKind = "passkey" | "magic-link" | "password" | "token";
export type PasskeyRow = {
  key: string;
  label: string;
  /**
   * default `passkey`
   */
  kind?: PasskeyKind;
  /**
   * ms
   */
  createdAt?: number;
  /**
   * ms; null or missing: never used
   */
  lastUsedAt?: number | null;
  /**
   * the way this session signed in with
   */
  current?: boolean;
};
export type PasskeysSpec = {
  rows?: PasskeyRow[];
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the sentence of a failed list
   */
  message?: string;
  /**
   * false leaves Retry out of a failed list
   */
  retry?: boolean;
  emptyTitle?: string;
  emptyText?: string;
  /**
   * false leaves the Register button out of the empty list
   */
  register?: boolean;
  /**
   * the page is not served over https: a warning with what to do next
   */
  insecure?: boolean;
  protectLast?: "refuse" | "confirm" | "off";
  confirm?: "dialog" | "type-name";
  refuseLook?: "button" | "note";
  markCurrent?: boolean;
  /**
   * days
   */
  neverUsedAfter?: number;
  /**
   * the clock (a test gives its own)
   */
  now?: () => number;
  /**
   * reject with an Error to refuse; its message is shown
   */
  remove?: (row: PasskeyRow) => Promise<unknown> | unknown;
  rename?: (row: PasskeyRow, label: string) => Promise<unknown> | unknown;
  /**
   * how many waiting rows (default: the rows shown last, else 3)
   */
  loadingRows?: number;
  /**
   * what the busy card says
   */
  loadingWords?: string;
};
export type Memory = {
  spec: PasskeysSpec;
  rows: PasskeyRow[];
  rowCount: number;
  height: number;
  card: import("./busy-card.js").BusyCard | null;
  drawn: boolean;
  pending: PasskeysSpec | null;
  wired: boolean;
  /**
   * the key being renamed
   */
  renaming: string | null;
  /**
   * a row's server refusal, by key
   */
  refused: Map<string, string>;
  /**
   * keys being removed or renamed
   */
  busy: Set<string>;
  announce: string;
};
/**
 * The options as the page wrote them: the spec first, then the attribute, then the default.
 * @param {HTMLElement} el @param {PasskeysSpec} spec
 */
export declare function passkeyOptions(
  el: HTMLElement,
  spec: PasskeysSpec,
): {
  protectLast: "refuse" | "confirm" | "off";
  confirm: "dialog" | "type-name";
  refuseLook: "button" | "note";
  markCurrent: boolean;
  neverUsedAfter: number;
};
/**
 * The order of the list: the one in use first, then the most recently used,
 * a way never used last.
 * @param {readonly PasskeyRow[]} rows
 * @returns {PasskeyRow[]}
 */
export declare function sortPasskeys(rows: readonly PasskeyRow[]): PasskeyRow[];
/**
 * Whole days a way has gone unused (since it was added when never used), or
 * null when it was used within `limit` days or the flag is off.
 * @param {PasskeyRow} row @param {number} now @param {number} limit
 * @returns {number | null}
 */
export declare function unusedDays(
  row: PasskeyRow,
  now: number,
  limit: number,
): number | null;
/**
 * Draw (or redraw) a passkey list.
 * @param {HTMLElement} el the root, a `.kp-passkeys`
 * @param {PasskeysSpec} spec
 */
export declare function setPasskeys(el: HTMLElement, spec: PasskeysSpec): void;
/**
 * Fill every `[data-kp-passkeys]` under `root` that carries its spec as JSON in
 * the attribute (a page that draws with setPasskeys() needs nothing).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachPasskeys(root?: ParentNode): () => void;
