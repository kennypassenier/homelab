// The kp-themes module chassis-rs serves at /static/kp/js/busy-card.js (kp-themes 11.0.0),
// copied from kp-themes js/busy-card.d.ts: chassis serves the module, not its types.
/** Every busy card, whatever component laid it. */
export declare const BUSY_CARD = ".kp-datatable__busy-card";
export type BusyCardOptions = {
  /**
   * what the card says under the spinner
   */
  words: string;
  /**
   *   the boxes the card covers, in viewport coordinates (an element's border box, or a rectangle); measured on every placement
   */
  area: () => readonly (
    | Element
    | {
        top: number;
        right: number;
        bottom: number;
        left: number;
      }
  )[];
  /**
   * the component's own class on the layer, beside `kp-datatable__busy-card`
   */
  className?: string;
  /**
   * arrive the theme's way (default true); false for a component's first render
   */
  arrive?: boolean;
};
export type BusyCard = {
  layer: HTMLElement;
  /**
   * measure and place it again
   */
  place: () => void;
  /**
   * let it leave the theme's way; settles once it is gone
   */
  close: () => Promise<void>;
  /**
   * take it out at once (a new loading state while it was leaving)
   */
  remove: () => void;
};
/**
 * Lay the busy card over a data area of `host`.
 * @param {HTMLElement} host
 * @param {BusyCardOptions} options
 * @returns {BusyCard}
 */
export declare function openBusyCard(
  host: HTMLElement,
  { words, area, className, arrive }: BusyCardOptions,
): BusyCard;
