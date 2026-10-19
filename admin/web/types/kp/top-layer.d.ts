/**
 * Where an element's box lies in window coordinates with its own `translate`
 * (and a `transform` that only translates) set aside: where the layout put
 * it, not where an arrival animation is drawing it this frame.
 *
 * Forest grows a menu, a popover, a combobox's list and a date picker's
 * panel out of their button from `translate: 0 -100%` (css/forest-register.css,
 * 2026-10-09). Measured on the frame it opened, the menu button's menu sat
 * its own height above where the layout put it, and the placement moved it
 * down by that much: in catalogue/overlays.html#menu-button the button
 * ended at 409 px and the menu started at 947 px, where formal starts it at
 * 413 px. Every placement measures through this instead, so every theme
 * places an overlay at its button whatever its arrival draws.
 *
 * @param {Element} element
 * @returns {DOMRect}
 */
export declare function layoutRect(element: Element): DOMRect;
/**
 * Raise an overlay that is being shown into the top layer.
 *
 * @param {HTMLElement} element the overlay, already visible (not `hidden`)
 * @param {(element: HTMLElement) => void} place puts it in window coordinates; called now and on every scroll and resize
 * @returns {() => void} lower — takes it out of the top layer, stops following, and removes what raising wrote
 */
export declare function raiseOverlay(
  element: HTMLElement,
  place: (element: HTMLElement) => void,
): () => void;
/**
 * Choose the side of its field a raised overlay opens on [fix-30].
 *
 * Measured on 2026-09-15: a combobox, a drawn select and a date picker at
 * the bottom of a 520px window each opened their overlay below the
 * window's edge (a list from y=486 to 604, a calendar from 512 to 788),
 * where neither a person nor a click could reach it; a fixed box outside
 * the viewport cannot be scrolled into view.
 *
 * The caller has just put the overlay below its field, where it always
 * opened; this reads that spot and keeps it when the overlay fits under
 * the window's bottom edge. When it does not and the room above the field
 * is larger, the overlay moves above the field at the same distance it hung
 * below it, and `data-kp-overlay-side="above"` says so, for a register that
 * wants to mirror something. When neither side holds it, the larger side
 * gets it with a `max-block-size` and the overlay scrolls itself.
 * Only the block axis: the inline placement stays the caller's, RTL
 * included. Horizontal writing modes only, as every caller is.
 *
 * @param {HTMLElement} element the overlay, in the top layer and placed below its field in window coordinates (its `top` written inline)
 * @param {{ top: number, bottom: number }} field the field's box in window coordinates
 * @returns {'below' | 'above'} the side taken
 */
export declare function placeBlockSide(
  element: HTMLElement,
  field: {
    top: number;
    bottom: number;
  },
): "below" | "above";
/**
 * Raise an overlay exactly where it was drawn: the spot and the width it
 * had in its container a moment ago, held against `anchor` while it is
 * open. For an overlay the stylesheet already places (a combobox's list at
 * its static position under the input), so the placement stays the
 * stylesheet's and a register that moves it is still obeyed.
 *
 * When the window has no room below, it opens above `field` instead
 * (placeBlockSide).
 *
 * @param {HTMLElement} element the overlay, already visible and laid out in place
 * @param {Element} anchor what it moves with — the control it belongs to
 * @param {Element} [field] what it opens under, and above when there is no room below — the input; the anchor when not given
 * @returns {() => void} lower
 */
export declare function raiseInPlace(
  element: HTMLElement,
  anchor: Element,
  field?: Element,
): () => void;
/**
 * Whether the overlay is in the top layer right now, and so placed in
 * window coordinates rather than against its containing block.
 *
 * @param {Element} element
 */
export declare const raised: (element: Element) => boolean;
