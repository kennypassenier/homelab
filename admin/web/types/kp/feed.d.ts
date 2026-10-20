/** An activity feed. */
export declare const FEED = "[data-kp-feed]";
/** Fired on the host, bubbling, when Retry is pressed on the failed plate or "Read it again" on a stale feed. */
export declare const FEED_RETRY_EVENT = "kp-feed-retry";
/** Fired on the host, bubbling, when "Load earlier" (or its Try again) is pressed. The page answers with `earlier: 'loading'` and then the rows. */
export declare const FEED_EARLIER_EVENT = "kp-feed-earlier";
/** Fired on the host, bubbling, when a row opens or closes: `{ key, open }`. */
export declare const FEED_OPEN_EVENT = "kp-feed-open";
/** The states a row can be in. */
export declare const FEED_ROW_STATES: readonly [
  "ok",
  "failed",
  "warn",
  "running",
  "info",
];
/** The states the feed can be in. */
export declare const FEED_STATES: readonly [
  "ready",
  "loading",
  "empty",
  "error",
];
/** The states of the "Load earlier" foot. */
export declare const FEED_EARLIER: readonly ["more", "loading", "error", "end"];
/** The zone a day is counted in, whatever the reader's (rule 52); the same as js/freshness.js. */
export declare const FEED_TIME_ZONE = "Europe/Brussels";
/** Rows younger than this show the ago line (time `auto` and `clock`) [ms]. */
export declare const FEED_RELATIVE_MS: number;
/** How often the day headings and the ago-or-moment choice are looked at again [ms]. */
export declare const FEED_TICK_MS = 30000;
export type FeedRowState = "ok" | "failed" | "warn" | "running" | "info";
export type FeedActor = {
  /**
   * who or what started it ("Claude", "Kenny", "nightly round")
   */
  name: string;
  /**
   * `person` and `agent` get a round chip; anything else (a schedule, the system) a square one
   */
  kind?: string;
};
export type FeedRow = {
  /**
   * stable id: the same key is the same row on every refresh
   */
  key: string;
  /**
   * what happened, in one line
   */
  title: string;
  /**
   * when, ms since the epoch
   */
  at: number;
  /**
   * its outcome; a row without one has an empty mark
   */
  state?: FeedRowState;
  actor?: FeedActor;
  /**
   * the muted line under the title
   */
  detail?: string;
  /**
   * where the title goes
   */
  href?: string;
  /**
   * lines that open under the row (a log excerpt); the title becomes the disclosure button
   */
  more?: string[];
};
export type FeedStale = {
  /**
   * why the refresh failed, in plain words
   */
  reason: string;
  /**
   * when the rows on screen were read
   */
  since?: number;
};
export type FeedModel = {
  state?: "ready" | "loading" | "empty" | "error";
  rows?: FeedRow[];
  /**
   * how many rows there are in all (exact); default the number given
   */
  total?: number;
  /**
   * the "Load earlier" foot
   */
  earlier?: "more" | "loading" | "error" | "end";
  /**
   * when the feed was read last, for the freshness line
   */
  updatedAt?: number;
  stale?: FeedStale | null;
  /**
   * why it could not be read (the failed plate's sentence)
   */
  error?: string;
  /**
   * why there is nothing (the empty plate's sentence)
   */
  emptyText?: string;
};
export type FeedHandle = {
  element: HTMLElement;
  set: (model: FeedModel) => void;
  /**
   * the keys of the rows that are open
   */
  openKeys: () => string[];
  /**
   * open exactly these rows (or every row that has lines), close the rest; fires no event
   */
  setOpen: (keys: readonly string[] | "all") => void;
  /**
   * draw again, with the options as they stand now
   */
  render: () => void;
};
/** The handle of an attached feed. @param {Element} element */
export declare const feedOf: (element: Element) => FeedHandle | null;
/**
 * Give a feed its rows, or its state.
 * @param {Element} element @param {FeedModel} model
 */
export declare function setFeed(element: Element, model: FeedModel): void;
/**
 * Wire every feed under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number, timeZone?: string }} [options] `now` so a test never reads the real clock
 * @returns {() => void} detach
 */
export declare function attachFeeds(
  root?: ParentNode,
  {
    now,
    timeZone,
  }?: {
    now?: () => number;
    timeZone?: string;
  },
): () => void;
