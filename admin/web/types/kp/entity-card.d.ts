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
/** Fired on the card when its star is pressed: `{ key, starred }`. */
export declare const ENTITY_STAR_EVENT = "kp-entity-star";
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
    tone?: "success" | "neutral" | "warning" | "destructive";
    text?: string;
  }[];
  actions?: {
    id: string;
    label: string;
    danger?: boolean;
    disabled?: string;
  }[];
  /**
   * the readings of the trend row, oldest first (a 24-hour CPU line); fewer than two say "no line yet"
   */
  trend?: number[];
  /**
   * the trend row's label ("CPU 24 h"); the package's word "Trend" when absent
   */
  trendLabel?: string;
  /**
   * the current value beside the line ("34%"); the last reading when absent
   */
  trendText?: string;
  /**
   * a tile's link: the whole card opens it
   */
  href?: string;
  /**
   * the link opens a new tab (a glyph shows, and a screen reader is told)
   */
  external?: boolean;
  /**
   * a tile's description; it wraps whole
   */
  description?: string;
  /**
   * a tile's small facts, one per line, at the foot
   */
  lines?: string[];
  /**
   * a tile's typed search: its first occurrence in the name (any case) is wrapped in a `<mark class="kp-entity__match">`; the rest is plain text, and nothing is parsed as HTML [wish-1120]
   */
  match?: string;
  /**
   * the favourite star is on; a refill with it changes the star in place
   */
  starred?: boolean;
  status?: "ready" | "loading" | "empty" | "error";
  /**
   * why a card in `error` failed
   */
  error?: string;
};
export type EntityEmptyOptions = {
  /**
   * the plate's heading
   */
  title?: string;
  /**
   * the line under it
   */
  message?: string;
  /**
   * the action's words
   */
  actionLabel?: string;
  /**
   * `none`: the plate has no action
   */
  action?: "none";
  /**
   * what the action does, in place of firing `kp-entity-new`
   */
  onAction?: (event: MouseEvent) => void;
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
 * @param {EntityEmptyOptions} [empty] how the plate reads when the list is empty (see `setEntityGridState`)
 */
export declare function setEntityGrid(
  grid: HTMLElement,
  list: readonly EntityData[],
  empty?: EntityEmptyOptions,
): void;
/**
 * Put the whole grid in a state: `loading` (skeleton cards, as many as `options.count` or the cards it had, 6 at first, and the
 * busy card over them), `empty` ("No stacks yet" with its action) or `error` (the failed plate with Retry); a tile grid says "apps".
 * @param {HTMLElement} grid
 * @param {'loading' | 'empty' | 'error'} state
 * @param {{ count?: number, words?: string } & EntityEmptyOptions} [options] `words` is the busy card's line. The empty plate's action: `action: 'none'`
 * (or `data-kp-entity-empty-action="none"` on the grid) leaves it out, `actionLabel` words it, and `onAction` runs instead of the
 * `kp-entity-new` event
 */
export declare function setEntityGridState(
  grid: HTMLElement,
  state: "loading" | "empty" | "error",
  options?: {
    count?: number;
    words?: string;
  } & EntityEmptyOptions,
): void;
/**
 * Give every grid its options (the least card width) and keep the cards in step with them: a changed attribute draws every card again.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachEntityCards(root?: ParentNode): () => void;
