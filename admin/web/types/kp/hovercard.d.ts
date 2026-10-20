// The kp-themes module chassis-rs serves at /static/kp/js/hovercard.js (kp-themes 11.1.1),
// copied from kp-themes js/hovercard.d.ts: chassis serves the module, not its types.
/** The attribute that makes an element an anchor; its value is the card's text when not empty. */
export declare const HOVERCARD_ATTRIBUTE = "data-kp-hovercard";
/** Dispatched on the anchor, bubbling, when the card opens or closes: `{ open, card, anchor }`. */
export declare const HOVERCARD_EVENT = "kp-hovercard";
/** Written on the one shared card node. */
export declare const HOVERCARD_CARD = "data-kp-hovercard-card";
export type HoverCardContent =
  string | Node | readonly Node[] | null | undefined | false;
export type HoverCardOptions = {
  /**
   * What an anchor is. Default `[data-kp-hovercard]`; per call.
   */
  selector?: string;
  /**
   * Runs when the card is about to open or to move to another anchor; the words for this anchor right now.
   */
  fill?: (anchor: HTMLElement, card: HTMLElement) => HoverCardContent;
  /**
   * Pointer only; keyboard focus opens at once. Default 300. Per anchor `data-kp-open-delay`.
   */
  openDelayMs?: number;
  /**
   * Default 100. Per anchor `data-kp-close-delay`.
   */
  closeDelayMs?: number;
  /**
   * Default true. Per anchor `data-kp-close-on-escape="false"`.
   */
  closeOnEscape?: boolean;
  /**
   * A tap opens and closes the card. Default true; per anchor `data-kp-touch="off" | "tap"`.
   */
  touch?: boolean;
  /**
   * A class added to the card, for a page's own measure (it lives in <body>).
   */
  cardClass?: string;
};
export type HoverCardHandle = (() => void) & {
  card: HTMLElement;
  open: (anchor: HTMLElement) => void;
  hide: () => void;
  refresh: () => void;
  anchor: () => HTMLElement | null;
};
/**
 * Attach the page's hover card to `root`.
 *
 * @param {Document | Element} [root]
 * @param {HoverCardOptions} [options]
 * @returns {HoverCardHandle} detach, with the handle's members on it
 */
export declare function attachHoverCards(
  root?: Document | Element,
  options?: HoverCardOptions,
): HoverCardHandle;
