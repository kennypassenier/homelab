// The kp-themes module chassis-rs serves at /static/kp/js/palette.js (kp-themes 7.2.0),
// copied from kp-themes js/palette.d.ts: chassis serves the module, not its types.
/** The attribute that opens a palette or a sheet on a press [gap-12]. */
export declare const OPENER = "[data-kp-palette-open]";
/**
 * Close a modal dialog on a press outside its box [scope-80].
 *
 * A modal `<dialog>` paints its backdrop as part of itself, so a press on
 * the dimmed page lands on the dialog element with coordinates outside its
 * border box. Both the press and the release have to fall outside: a drag
 * that selects the query and lets go past the box is not a click outside.
 * `dialog.close()` is what Escape does too, so focus goes back to the
 * opener the same way, and the `close` event runs every channel's own
 * bookkeeping. Returns the function that takes the listeners off.
 *
 * @param {HTMLDialogElement} dialog
 * @returns {() => void}
 */
export declare function closeOnOutsidePress(
  dialog: HTMLDialogElement,
): () => void;
/** An empty `<kbd>` inside an opener that the module fills with the hotkey [scope-48]. */
export declare const KEYS_SLOT = "[data-kp-palette-keys]";
/** Whether this is a Mac, where the modifier is ⌘ rather than Ctrl. */
export declare function isMac(): boolean;
/** Fired on the palette when a command is chosen, cancelable. A contract value [TH26]: `{ value, option, href }`. */
export declare const RUN_EVENT = "kp-palette-run";
/** Fired on the palette or the sheet when it opens or closes: `{ open }`. */
export declare const OPEN_EVENT = "kp-palette-open";
export type Matcher = (optionText: string, query: string) => boolean;
/** @typedef {(optionText: string, query: string) => boolean} Matcher */
/** @type {Record<string, Matcher>} */
export declare const MATCHERS: Record<string, Matcher>;
export type PaletteHandle = {
  element: HTMLDialogElement;
  open: (query?: string) => void;
  close: () => void;
  /**
   * re-filter after the consumer changed the commands
   */
  refresh: () => void;
};
/** The handle for an attached palette or sheet. @param {Element} element */
export declare function palette(element: Element): PaletteHandle | null;
/**
 * Attach every palette and shortcut sheet under `root`.
 *
 * @param {ParentNode} root
 * @param {{ hotkey?: string | null, sheetKey?: string | null, match?: keyof typeof MATCHERS | Matcher, clearOnClose?: boolean, closeOnRun?: boolean, typingSelector?: string }} [options]
 *   Defaults; per element: `data-kp-hotkey` (a letter, or "none"), `data-kp-primary` (this one answers the key when there are several), `data-kp-match` (`substring` by default, or `subsequence`, `prefix`), `data-kp-clear-on-close="false"`, `data-kp-close-on-run="false"`.
 * @returns {(() => void) & { handles: PaletteHandle[] }} detach
 */
export declare function attachPalettes(
  root?: ParentNode,
  {
    hotkey,
    sheetKey,
    match,
    clearOnClose,
    closeOnRun,
    typingSelector,
  }?: {
    hotkey?: string | null;
    sheetKey?: string | null;
    match?: keyof typeof MATCHERS | Matcher;
    clearOnClose?: boolean;
    closeOnRun?: boolean;
    typingSelector?: string;
  },
): (() => void) & {
  handles: PaletteHandle[];
};
