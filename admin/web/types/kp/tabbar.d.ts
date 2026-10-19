// The kp-themes module chassis-rs serves at /static/kp/js/tabbar.js (kp-themes 11.0.0),
// copied from kp-themes js/tabbar.d.ts: chassis serves the module, not its types.
/** A tab bar. */
export declare const TABBAR = ".kp-tabbar";
/** The root property the bar's height is published on while the bar is shown. */
export declare const TABBAR_HEIGHT_PROPERTY = "--kp-tabbar-height";
/**
 * Write a count on a tab: all its digits (12 is 12 and 1534 is 1534, never `99+`
 * or `1.5k`: Kenny, 2026-10-03, a counter shows the real number), nothing and hidden at
 * 0 or `null`. The badge is made when the tab has none. A tab that has a
 * count is the one place the bar speaks for something that moved, so the
 * count is also on the tab as `data-kp-count`, for a register or a test.
 *
 * @param {Element} tab a `.kp-tabbar__tab`
 * @param {number | null} count
 * @returns {HTMLElement | null} the badge
 */
export declare function setTabbarBadge(
  tab: Element,
  count: number | null,
): HTMLElement | null;
/**
 * Make one tab the current page: `aria-current="page"` on it, off the others.
 * A `data-kp-tab` that matches no tab leaves every tab not current, which is
 * right for a page that lives behind the More tab (that tab marks itself).
 *
 * @param {Element} bar a `.kp-tabbar`
 * @param {string | null} id the `data-kp-tab` of the current tab
 */
export declare function setTabbarCurrent(bar: Element, id: string | null): void;
/**
 * Put a tab to wait: `aria-busy` on it and the theme's own spinner
 * (`.kp-spinner`) in the icon's place, which has its size, so the tab does not
 * move. Not busy puts the icon back.
 *
 * @param {Element} tab a `.kp-tabbar__tab`
 * @param {boolean} busy
 */
export declare function setTabbarBusy(tab: Element, busy: boolean): void;
/**
 * Wire every tab bar under `root`. Idempotent.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTabbars(root?: ParentNode): () => void;
