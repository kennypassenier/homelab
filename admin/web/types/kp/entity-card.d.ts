/** The grids this module looks after. */
export declare const ENTITY_GRID = "[data-kp-entity-grid]";
/** A card this module looks after. */
export declare const ENTITY_CARD = "[data-kp-entity]";
/** Fired on the card when an action is pressed: `{ key, id }`. */
export declare const ENTITY_ACTION_EVENT = "kp-entity-action";
/** Fired on the card when a flag is picked in the "+N" menu: `{ key, flag }`. */
export declare const ENTITY_FLAG_EVENT = "kp-entity-flag";
/** Fired on the card when its tick changes: `{ key, ticked }`. */
export declare const ENTITY_TICK_EVENT = "kp-entity-tick";
/** Fired on the card when its Retry is pressed: `{ key }`. */
export declare const ENTITY_RETRY_EVENT = "kp-entity-retry";
/** Fired on the grid when its action is pressed. */
export declare const ENTITY_NEW_EVENT = "kp-entity-new";
/** Fired on the grid when its Retry is pressed. */
export declare const ENTITY_GRID_RETRY_EVENT = "kp-entity-grid-retry";
export type EntityData = {
  /**
   * a stable id, for selection, undo and a live refresh
   */
  key: string;
  name: string;
  /**
   * a small id beside the name
   */
  vmid?: string;
  state?: {
    label: string;
    tone?: "success" | "warning" | "destructive" | "info";
  };
  figures?: {
    value: string;
    label: string;
  }[];
  flags?: {
    label: string;
    tone?: "success" | "warning" | "destructive" | "info";
  }[];
  meters?: {
    label: string;
    value: number | null;
    mark?: number;
    tone?: "warning" | "destructive";
    text?: string;
  }[];
  actions?: {
    id: string;
    label: string;
    danger?: boolean;
    disabled?: string;
  }[];
  status?: "ready" | "loading" | "empty" | "error";
  /**
   * why a card in `error` failed
   */
  error?: string;
};
/**
 * Draw one card from its data. Only what differs from the last draw is touched.
 * @param {HTMLElement} card
 * @param {EntityData} data
 */
export declare function setEntity(card: HTMLElement, data: EntityData): void;
/**
 * Draw a grid's cards from a list; a card whose key is already there changes in place, a gone key leaves, order follows the list.
 * @param {HTMLElement} grid
 * @param {readonly EntityData[]} list
 */
export declare function setEntityGrid(
  grid: HTMLElement,
  list: readonly EntityData[],
): void;
/**
 * Put the whole grid in a state: `loading` (skeleton cards, as many as `options.count` or the cards it had, 6 at first, and the
 * busy card over them), `empty` ("No stacks yet" with its action) or `error` (the failed plate with Retry).
 * @param {HTMLElement} grid
 * @param {'loading' | 'empty' | 'error'} state
 * @param {{ count?: number, message?: string, title?: string, actionLabel?: string }} [options]
 */
export declare function setEntityGridState(
  grid: HTMLElement,
  state: "loading" | "empty" | "error",
  options?: {
    count?: number;
    message?: string;
    title?: string;
    actionLabel?: string;
  },
): void;
/**
 * Give every grid its options (the least card width) and keep the cards in step with them: a changed attribute draws every card again.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachEntityCards(root?: ParentNode): () => void;
