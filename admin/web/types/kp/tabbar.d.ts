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
/** A tab of the underline row. */
export declare const TAB = ".kp-tab";
/** The tones a count may carry; each is a plate the badge already has. */
export declare const TAB_COUNT_TONES: readonly [
  "neutral",
  "info",
  "success",
  "warning",
  "error",
];
export type TabCountTone = "neutral" | "info" | "success" | "warning" | "error";
/** @typedef {'neutral' | 'info' | 'success' | 'warning' | 'error'} TabCountTone */
/** Fired on a tab (bubbling) when what its count shows changed: `{ tab, count, state, tone }`. */
export declare const TAB_COUNT_EVENT = "kp-tab-count";
/**
 * Draw the count part of one tab from its attributes. Made when missing, kept in place otherwise.
 *
 * @param {Element} tab a `.kp-tab`
 * @returns {HTMLElement | null} the count part, null while the tab has none
 */
export declare function drawTabCount(tab: Element): HTMLElement | null;
/**
 * Write the count of a tab of the underline row: every digit of it (12 is 12 and 1534 is 1534, never `9+` or `1.5k`), nothing at
 * `null` (a tab that has not been counted is not a tab with 0), nothing at 0 unless `zero: 'show'`. `'loading'` draws a skeleton
 * the width of the last number, `'error'` a dash with the words "could not be counted" for a screen reader. An option left out keeps
 * what the tab already says, so a refresh is `setTabCount(tab, 13)`; `null` as an option takes it away. The call writes the
 * `data-kp-tab-count…` attributes and draws, so markup and call are one thing.
 *
 * @param {Element} tab a `.kp-tab`
 * @param {number | null | 'loading' | 'error'} count
 * @param {{ tone?: TabCountTone | null, unit?: string | null, zero?: 'show' | 'hide' | null, showUnit?: boolean | null }} [options]
 *        `unit` is the singular and the plural, "error,errors"
 * @returns {HTMLElement | null} the count part
 */
export declare function setTabCount(
  tab: Element,
  count: number | null | "loading" | "error",
  options?: {
    tone?: TabCountTone | null;
    unit?: string | null;
    zero?: "show" | "hide" | null;
    showUnit?: boolean | null;
  },
): HTMLElement | null;
/**
 * Keep the count of every tab under `root` drawn from its attributes: now, and whenever an attribute of the count changes or a
 * tab comes. Idempotent.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTabCounts(root?: ParentNode): () => void;
/**
 * Wire every tab bar under `root`. Idempotent.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachTabbars(root?: ParentNode): () => void;
