/** The attribute a register link carries, with the theme it serves as its value. A contract value. */
export declare const REGISTER_ATTRIBUTE = "data-kp-register";
/** Where a register is served from by default; `{theme}` is replaced by the name. */
export declare const REGISTER_PATTERN = "/css/{theme}-register.css";
/** Dispatched on the root, bubbling, when a register has loaded: `{ theme, href }`. */
export declare const REGISTER_LOAD_EVENT = "kp-register-load";
/** Dispatched on the root, bubbling, when a register failed to load: `{ theme, href }`. The theme stays what it was. */
export declare const REGISTER_ERROR_EVENT = "kp-register-error";
export type LazyRegisterOptions = {
  /**
   * where a register is served from, with `{theme}` in it (default REGISTER_PATTERN)
   */
  pattern?: string;
  /**
   * the document to insert into (default: the current one)
   */
  doc?: Document;
};
/**
 * The URL of a theme's register.
 *
 * @param {string} theme
 * @param {{ pattern?: string, doc?: Document }} [options]
 * @returns {string} absolute, resolved against the document
 */
export declare function registerHref(
  theme: string,
  {
    pattern,
    doc,
  }?: {
    pattern?: string;
    doc?: Document;
  },
): string;
/**
 * The link in the document that serves `theme`'s register: one carrying
 * REGISTER_ATTRIBUTE for it, or else a plain stylesheet link at its URL.
 *
 * @param {string} theme
 * @param {LazyRegisterOptions} [options]
 * @returns {HTMLLinkElement | null}
 */
export declare function registerLink(
  theme: string,
  { pattern, doc }?: LazyRegisterOptions,
): HTMLLinkElement | null;
/**
 * The themes whose register links are in the document, in document order.
 *
 * @param {Document} [doc]
 * @returns {string[]}
 */
export declare function registersPresent(doc?: Document): string[];
/**
 * Whether `theme`'s register is in the document and has loaded.
 *
 * @param {string} theme
 * @param {LazyRegisterOptions} [options]
 */
export declare function registerLoaded(
  theme: string,
  { pattern, doc }?: LazyRegisterOptions,
): boolean;
/**
 * Make sure `theme`'s register is in the document, and resolve when it has
 * loaded. A link already there — the no-flash snippet's, a hand-written
 * one, an earlier call's — is reused, so a register is fetched once. A
 * failed load is forgotten and its link removed, so the next call tries
 * again.
 *
 * @param {string} theme
 * @param {LazyRegisterOptions} [options]
 * @returns {Promise<HTMLLinkElement>}
 */
export declare function ensureRegister(
  theme: string,
  { pattern, doc }?: LazyRegisterOptions,
): Promise<HTMLLinkElement>;
/**
 * Keep the document's registers in step with its theme.
 *
 * @param {{ pattern?: string, hold?: boolean, prune?: boolean, root?: Element }} [options]
 *   pattern: where a register is served from, with `{theme}` in it (default REGISTER_PATTERN) —
 *     the same value the no-flash snippet was given;
 *   hold: keep a change back until its register has loaded (default true). Off, the root flips at
 *     once and wears the new tokens without their register until the file lands;
 *   prune: remove the previous theme's register after a change (default false: kept, so that
 *     switching back fetches nothing);
 *   root: the element that wears the theme (default: theme-core's root).
 * @returns {() => void} detach: stop following theme changes and drop a held change; links already inserted stay
 */
export declare function attachLazyRegisters({
  pattern,
  hold,
  prune,
  root,
}?: {
  pattern?: string;
  hold?: boolean;
  prune?: boolean;
  root?: Element;
}): () => void;
