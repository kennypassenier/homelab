/** The lists this module looks after. */
export declare const STATUS_LIST = ".kp-statuslist";
/** The attribute that names a row, for setStatus(). */
export declare const STATUS_ID_ATTRIBUTE = "data-kp-status-id";
/** Fired on the list, bubbling, when a row changed state after attach: `{ item, id, state, previous }`. */
export declare const STATUS_EVENT = "kp-status-change";
/** Fired on the list, bubbling, when a Retry is pressed: `{ item, id }`. */
export declare const STATUS_RETRY_EVENT = "kp-status-retry";
/** Fired on the list, bubbling, when Resume is pressed on a paused calm row (cancelable): `{ item, id }`. */
export declare const STATUS_RESUME_EVENT = "kp-status-resume";
/** Fired on the list, bubbling, when Pause is pressed on an open calm row (cancelable): `{ item, id }`. */
export declare const STATUS_PAUSE_EVENT = "kp-status-pause";
/** Fired on the list, bubbling, when Undo takes a resume or a pause back: `{ item, id, action }`. */
export declare const STATUS_UNDO_EVENT = "kp-status-undo";
/** The states a row can be in. */
export declare const STATUS_STATES: readonly [
  "ok",
  "loading",
  "failed",
  "empty",
  "warn",
];
/** The states a row of the calm vocabulary can be in. */
export declare const CALM_STATUS_STATES: readonly [
  "open",
  "done",
  "paused",
  "unknown",
  "loading",
];
export type StatusState =
  | "ok"
  | "loading"
  | "failed"
  | "empty"
  | "warn"
  | "open"
  | "done"
  | "paused"
  | "unknown";
export type StatusRow = {
  /**
   * names the row for setStatus(); defaults to its label
   */
  id?: string;
  label: string;
  /**
   * the short line under the label: how long, the reason; give every row one so a reason arriving later moves nothing
   */
  detail?: string;
  /**
   * loading when absent
   */
  state?: StatusState;
  /**
   * what to do about it, written out; shown on a failed or warn row only [kpadopt4-rows]
   */
  fix?: string;
  /**
   * a paused calm row's day, `YYYY-MM-DD` [kpadopt6]
   */
  since?: string;
};
/**
 * Build a status list from rows. The result still needs attachStatusLists()
 * (on the element or its container) for the spoken parts and the Retry.
 *
 * @param {StatusRow[]} rows
 * @param {{ label?: string, retry?: boolean, single?: boolean, vocab?: 'standard' | 'calm' }} [options] `label`: the list's accessible name;
 *   `retry: false` leaves out the Retry button; `single`: one column; `vocab: 'calm'` the open, done, paused and unknown rows [kpadopt6]
 * @returns {HTMLUListElement}
 */
export declare function buildStatusList(
  rows: StatusRow[],
  {
    label,
    retry,
    single,
    vocab,
  }?: {
    label?: string;
    retry?: boolean;
    single?: boolean;
    vocab?: "standard" | "calm";
  },
): HTMLUListElement;
/**
 * Order and fold a calm list once, at the page's word: `data-kp-today-order="open-first"` puts the open rows first (nothing
 * moves afterwards, so a row that becomes done stays under the thumb), and `data-kp-today-paused="fold"` gathers the rows
 * paused for more than `data-kp-today-fold-after` days (30) into one line that opens them.
 * @param {Element} list
 */
export declare function settleToday(list: Element): void;
/**
 * Change one row. `key` is the row's `data-kp-status-id` or its index.
 *
 * @param {Element} list
 * @param {string | number} key
 * @param {{ state?: StatusState, label?: string, detail?: string, fix?: string, since?: string }} change
 * @returns {Element | null} the row, or null when there is no such row
 */
export declare function setStatus(
  list: Element,
  key: string | number,
  change: {
    state?: StatusState;
    label?: string;
    detail?: string;
    fix?: string;
    since?: string;
  },
): Element | null;
/**
 * How many rows of a list are in each state, for a summary line such as
 * "3 of 4 checks pass" [kpadopt4-rows]. Counts the rows as they are now.
 * @param {Element} list
 * @returns {Record<StatusState, number> & { total: number }}
 */
export declare function countStatus(list: Element): Record<
  StatusState,
  number
> & {
  total: number;
};
/**
 * Sync every `.kp-statuslist` under `root`, follow `data-state` changes later
 * and turn a press on a Retry into `kp-status-retry`. A list rendered after
 * this ran is attached by calling it on that subtree.
 *
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachStatusLists(root?: ParentNode): () => void;
/**
 * The state of a today card (a `.kp-card` around a calm list): `loading` lays the busy card over the rows, `empty` says
 * "Nothing planned today" over "That is fine, nothing is waiting", `error` is blame-free with a normal-size Retry (the card's
 * `kp-card-retry`). The words are the dictionary's `statusToday…`, or the page's own.
 * @param {HTMLElement} card
 * @param {'ready' | 'loading' | 'empty' | 'error'} state
 * @param {{ title?: string, text?: string, words?: string }} [words]
 */
export declare function setTodayState(
  card: HTMLElement,
  state: "ready" | "loading" | "empty" | "error",
  words?: {
    title?: string;
    text?: string;
    words?: string;
  },
): void;
