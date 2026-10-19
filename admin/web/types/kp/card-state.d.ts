/** The cards this module looks after. */
export declare const CARD_STATE = ".kp-card[data-kp-state]";
/** The four states, in the order a read goes through them. */
export declare const CARD_STATES: readonly [
  "loading",
  "empty",
  "error",
  "ready",
];
/** Fired on the card, bubbling, when its state changed after attach: `{ card, state, previous }`. */
export declare const CARD_STATE_EVENT = "kp-card-state";
/** Fired on the card, bubbling, when its Try again is pressed: `{ card }`. */
export declare const CARD_RETRY_EVENT = "kp-card-retry";
export type CardState = "loading" | "empty" | "error" | "ready";
export type CardStateOptions = {
  /**
   * what the loading card says under its spinner (default `cardLoading`, or the card's `data-kp-loading-words`)
   */
  words?: string;
  /**
   * the empty or error note's first line, in bold: "No appointments yet", "Could not list the files"
   */
  title?: string;
  /**
   * its sentence: why, or what fills it
   */
  text?: string;
  /**
   * a button or link under an empty note ("Add an appointment"); an error note always has Try again
   */
  action?: Node | null;
  /**
   * false leaves Try again out of an error note (a read the page cannot repeat)
   */
  retry?: boolean;
};
/**
 * Put a card in a state. The options are kept for the card, so a later
 * `data-kp-state` change written by a framework builds the same note.
 * @param {HTMLElement} card a `.kp-card`
 * @param {CardState} state
 * @param {CardStateOptions} [options]
 * @returns {CardState}
 */
export declare function setCardState(
  card: HTMLElement,
  state: CardState,
  options?: CardStateOptions,
): CardState;
/**
 * Sync every card with a `data-kp-state` under `root`, follow the attribute
 * later, and turn a press on Try again into `kp-card-retry`. A card rendered
 * after this ran is attached by calling it on that subtree.
 * @param {ParentNode} [root]
 * @returns {() => void} detach: stops following; a loading card's busy card is taken out
 */
export declare function attachCardStates(root?: ParentNode): () => void;
