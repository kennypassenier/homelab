/** The chord's wait for its second key, in ms (homelab's tested value). */
export declare const CHORD_MS = 1500;
/** How long the "no shortcut on g x" line stays, in ms. */
export declare const UNKNOWN_MS = 1500;
/** A sheet that is filled from the registry. */
export declare const SHORTCUTS_LIVE_SHEET =
  "[data-kp-shortcuts][data-kp-shortcuts-live]";
/** Fired on the document, bubbling, when a registered shortcut ran: `{ keys, what }`. */
export declare const SHORTCUT_EVENT = "kp-shortcut-run";
/** Fired on the document when a chord starts or stops waiting for its next key: `{ keys, waiting, ms }`. */
export declare const SHORTCUT_PENDING_EVENT = "kp-shortcut-pending";
/** Where the chord's cap stands: the middle of the screen (default) or the middle of its foot. */
export declare const SHORTCUT_HINT_PLACES: readonly ["centre", "bottom-centre"];
/** Keys that need AltGr on an AZERTY keyboard. */
export declare const ALTGR_KEYS: readonly string[];
export type Shortcut = {
  /**
   * "g s", "Mod K", "?", "j"
   */
  keys: string;
  /**
   * the sheet's words for it
   */
  what: string;
  /**
   * the sheet's heading for it (default "Shortcuts")
   */
  group?: string;
  /**
   * called when the keys are pressed; returning `false` leaves the browser's own handling alone
   */
  run: (event: KeyboardEvent) => unknown;
  /**
   * false: the key does nothing now and the sheet shows it dimmed
   */
  when?: () => boolean;
  /**
   * the sheet's sentence for why it is not available now
   */
  reason?: string;
};
export type EngineOptions = {
  /**
   * the wait for a chord's next key (default 1500)
   */
  chordMs?: number;
  /**
   * what to do with a key that needs AltGr on AZERTY (default `warn`)
   */
  layoutGuard?: "warn" | "refuse" | "off";
  /**
   * where letters never fire (default the palette's)
   */
  typingSelector?: string;
  /**
   * show "No shortcut on g x" after a chord that does nothing (default false)
   */
  unknownHint?: boolean;
  /**
   * throw on a conflict instead of logging it (default: on localhost)
   */
  dev?: boolean;
  /**
   * where warnings go (default the console)
   */
  onWarn?: (message: string) => void;
  /**
   * the sheet's group order; others follow in registration order
   */
  groupOrder?: string[];
  /**
   * where the chord's cap stands (default: the root's
   * `data-kp-shortcuts-hint`, else `centre`)
   */
  hintPlace?: "centre" | "bottom-centre";
};
/**
 * Change the engine's defaults for the whole page.
 * @param {EngineOptions} next
 */
export declare function configureShortcuts(next: EngineOptions): void;
export type Step = {
  /**
   * sorted modifier names: `alt`, `mod`, `shift`
   */
  mods: string[];
  /**
   * the key, as `KeyboardEvent.key` spells it
   */
  key: string;
};
/**
 * @typedef {object} Step
 * @property {string[]} mods sorted modifier names: `alt`, `mod`, `shift`
 * @property {string} key the key, as `KeyboardEvent.key` spells it
 */
/**
 * Read "g s" or "Mod K" into steps.
 * @param {string} keys
 * @returns {Step[]}
 */
export declare function parseShortcutKeys(keys: string): Step[];
/** The keys as the sheet spells them. @param {string} keys */
export declare function displayShortcutKeys(keys: string): string;
export type Entry = {
  shortcut: Shortcut;
  steps: Step[];
  normal: string;
  scope: string | Element;
  order: number;
};
/**
 * Whether the engine would take this key: a chord waits for its next key, or a registered shortcut that is on screen
 * starts with it (a letter not while a person types in a field). For a page embedded in another, which hands its
 * keys up to the page around it unless they are its own.
 * @param {KeyboardEvent} event
 * @returns {boolean}
 */
export declare function shortcutClaims(event: KeyboardEvent): boolean;
/**
 * Register a list of shortcuts under a scope.
 *
 * @param {string | Element} scope a name (the page's keys) or an element (keys that live while it is on screen)
 * @param {readonly Shortcut[]} list
 * @param {EngineOptions} [options] overrides the page's defaults for this list
 * @returns {() => void} unregister
 */
export declare function registerShortcuts(
  scope: string | Element,
  list: readonly Shortcut[],
  options?: EngineOptions,
): () => void;
/**
 * The registered shortcuts, for a consumer's own listing.
 * @returns {{ keys: string, what: string, group: string, scope: string, available: boolean }[]}
 */
export declare function listShortcuts(): {
  keys: string;
  what: string;
  group: string;
  scope: string;
  available: boolean;
}[];
/**
 * Write the sheet's body from the registry: groups of rows, the scope's own keys first, the palette's and the sheet's
 * keys in a fixed first group, a dimmed row for a key that is not available now, an empty plate when nothing is
 * registered.
 *
 * @param {HTMLElement} sheet
 */
export declare function renderShortcutSheet(sheet: HTMLElement): void;
/**
 * Keep every live sheet under `root` filled from the registry, and fresh each time it opens.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachShortcuts(root?: ParentNode): () => void;
