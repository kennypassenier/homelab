// The kp-themes module chassis-rs serves at /static/kp/js/menu-button.js (kp-themes 10.0.0),
// copied from kp-themes js/menu-button.d.ts: chassis serves the module, not its types.
/** A menu button's wrapper: a button, then its `[role=menu]`. */
export declare const MENU_BUTTON = "[data-kp-menu-button]";
/** Fired on the wrapper, bubbling, when the menu opened. */
export declare const MENU_OPEN_EVENT = "kp-menu-open";
/** Fired on the wrapper, bubbling, when the menu closed. */
export declare const MENU_CLOSE_EVENT = "kp-menu-close";
/** Fired on the wrapper, bubbling, when an entry was picked: `{ item, value }`. Cancelable: preventing it keeps the menu open. */
export declare const MENU_SELECT_EVENT = "kp-menu-select";
export type Strings = import("./strings.js").Strings;
export type MenuItem = {
  /**
   * what the action is called
   */
  label: string;
  /**
   * what it does, in one line
   */
  hint?: string;
  /**
   * a link rather than a button
   */
  href?: string;
  /**
   * with `href`: the file name to save under
   */
  download?: string;
  /**
   * why it cannot be used now; the entry stays, keeps its hint, and says this under it
   */
  disabled?: string | null;
  /**
   * a destructive action, in the destructive colour
   */
  danger?: boolean;
  /**
   * more attributes for the entry (`null` removes one)
   */
  attrs?: Record<string, string | null>;
  /**
   * what `kp-menu-select` reports (`data-kp-value`); the label when absent
   */
  value?: string;
};
export type MenuGroup = {
  /**
   * its heading
   */
  group: string;
  items: MenuItem[];
};
export type MenuDecorateInfo = {
  kind: "menu-button" | "menu-item";
  host: HTMLElement;
  key?: string;
  index?: number;
  label?: string;
  value?: string;
};
export type MenuButtonOptions = {
  /**
   * Called with the button and every entry the menu builds,
   * each time it builds one, so the consumer can mark them [R-DRIVE].
   */
  decorate?: (part: HTMLElement, info: MenuDecorateInfo) => void;
  /**
   * any of the dictionary's `menu…` words, for these menus only
   */
  strings?: Partial<Strings>;
};
export type MenuState = {
  wrapper: HTMLElement;
  button: HTMLElement;
  menu: HTMLElement;
  decorate: ((part: HTMLElement, info: MenuDecorateInfo) => void) | undefined;
  strings: Partial<Strings> | undefined;
  /**
   * what the menu shows now, as `menuSignature` writes it
   */
  drawn: string;
  /**
   * a fill that waits for the menu to close
   */
  pending: MenuGroup[] | "loading" | null;
  /**
   * takes a menu raised over a clipping container out of the top layer again
   */
  lower?: (() => void) | null;
  open: boolean;
  empty: boolean;
  /**
   * what an open menu listens to, undone when it closes
   */
  off: (() => void)[];
};
/**
 * What a fill shows, as one string: a fill that shows the same as the menu
 * does now is skipped.
 * @param {MenuGroup[] | 'loading'} groups
 * @returns {string}
 */
export declare const menuSignature: (groups: MenuGroup[] | "loading") => string;
/**
 * Where a key moves the focus in an open menu, or null when the key is not
 * the menu's: ↓ and ↑ step and wrap, Home and End go to the ends, and a
 * printable letter goes to the next entry after `at` whose label starts with
 * it (wrapping, the entry at `at` itself last). `at` is -1 when no entry has
 * the focus.
 * @param {{ key: string, ctrlKey?: boolean, metaKey?: boolean, altKey?: boolean }} event
 * @param {number} at
 * @param {readonly string[]} labels every entry's label, in order
 * @returns {number | null}
 */
export declare function menuKeyTarget(
  event: {
    key: string;
    ctrlKey?: boolean;
    metaKey?: boolean;
    altKey?: boolean;
  },
  at: number,
  labels: readonly string[],
): number | null;
/** The space an open menu keeps from the screen's edges, in px. */
export declare const MENU_GUTTER = 8;
export type Edges = {
  left: number;
  top: number;
  right: number;
  bottom: number;
};
export type Covered = Edges;
/** @typedef {{ left: number, top: number, right: number, bottom: number }} Edges */
/** How far in from each screen edge the page has something stuck: coveredEdges(). @typedef {Edges} Covered */
/**
 * Where an open menu goes so that all of it is on the screen (Kenny,
 * 2026-10-05: a menu under a button at the left edge opened off the
 * screen). Pure: boxes in, box out, all in viewport px.
 *
 * Across, the place the stylesheet gave it (under the button, at its end
 * edge; spanning the header's buttons on a phone) is kept when it fits;
 * else it lines up with the button's start edge, else with its end edge,
 * else it is pushed inside the room. Down, it hangs under the button when
 * it fits there, else over it when it fits there, else on the roomier side
 * at that side's height, scrolling inside itself.
 *
 * @param {object} at
 * @param {Edges} at.button the button's box
 * @param {Edges} at.menu where the stylesheet put the menu
 * @param {number} at.height the menu's whole height, within the stylesheet's own cap
 * @param {number} at.gap the space between the button and the menu
 * @param {Edges} at.room the screen less its gutter and what is stuck over its edges
 * @returns {{ left: number, top: number, width: number | null, height: number | null, side: 'below' | 'above' }}
 *   `width` and `height` are the caps to set, null where the menu fits whole
 */
export declare function menuPlacement({
  button,
  menu,
  height,
  gap,
  room,
}: {
  button: Edges;
  menu: Edges;
  height: number;
  gap: number;
  room: Edges;
}): {
  left: number;
  top: number;
  width: number | null;
  height: number | null;
  side: "below" | "above";
};
/**
 * Open a menu button's menu and put the focus on its first (or last) entry;
 * `none` leaves the focus where it is (a page showing the menu on its own,
 * not the reader asking for it).
 * @param {Element} wrapper
 * @param {{ focus?: 'first' | 'last' | 'none' }} [options]
 */
export declare function openMenu(
  wrapper: Element,
  {
    focus,
  }?: {
    focus?: "first" | "last" | "none";
  },
): void;
/**
 * Close it; with `focus`, the focus goes back to the button. A fill that
 * waited while it was open is drawn now.
 * @param {Element} wrapper
 * @param {{ focus?: boolean }} [options]
 */
export declare function closeMenu(
  wrapper: Element,
  {
    focus,
  }?: {
    focus?: boolean;
  },
): void;
/**
 * Give a menu button its entries. Skipped when they show the same as now;
 * while the menu is open the change waits until it closes, so a live refill
 * never moves an entry under the pointer or the focus. `'loading'` shows one
 * disabled entry at the final row height. A wrapper not attached yet is
 * wired (without listeners) so the fill is kept.
 * @param {Element} wrapper
 * @param {MenuGroup[] | 'loading'} groups
 */
export declare function setMenu(
  wrapper: Element,
  groups: MenuGroup[] | "loading",
): void;
/**
 * Wire every menu button under `root`: the button opens and closes the menu,
 * and the menu answers the keys of a menu (APG menu button). A disabled
 * entry (`aria-disabled`) keeps the focus, so its reason is read; it does
 * nothing. Idempotent.
 * @param {ParentNode} [root]
 * @param {MenuButtonOptions} [options]
 * @returns {() => void} detach
 */
export declare function attachMenuButtons(
  root?: ParentNode,
  { decorate, strings }?: MenuButtonOptions,
): () => void;
