/** A thread the page fills with setThread(). */
export declare const THREAD = "[data-kp-thread]";
/** Fired on the root when Retry (a failed list, or a message that was not sent) is pressed: `{ key? }`. */
export declare const THREAD_RETRY_EVENT = "kp-thread-retry";
/** Fired on the root when the Add button of an empty thread is pressed. */
export declare const THREAD_ADD_EVENT = "kp-thread-add";
/** Fired on the root when a message's own action is pressed: `{ key, id }`. */
export declare const THREAD_ACTION_EVENT = "kp-thread-action";
/** Fired on the root when a message is folded or shown whole: `{ key, expanded }`. */
export declare const THREAD_FOLD_EVENT = "kp-thread-fold";
/** The zone every time is written in. */
export declare const THREAD_TIME_ZONE = "Europe/Brussels";
export type ThreadAttachment = {
  name: string;
  /**
   * bytes
   */
  size?: number;
  href?: string;
};
export type ThreadMessage = {
  key: string;
  dir: "in" | "out" | "system";
  /**
   * ms
   */
  at: number;
  text: string;
  /**
   * who wrote it (the mark and the header); `out` says "You" without it
   */
  sender?: string;
  /**
   * the sender's photo
   */
  src?: string;
  attachments?: ThreadAttachment[];
  status?: "sent" | "sending" | "failed";
  /**
   * why a failed message was not sent
   */
  error?: string;
};
export type ThreadSpec = {
  messages?: ThreadMessage[];
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the sentence of a failed thread
   */
  message?: string;
  /**
   * false leaves Retry out of a failed thread
   */
  retry?: boolean;
  emptyTitle?: string;
  emptyText?: string;
  /**
   * false leaves the Add button out of the empty thread
   */
  add?: boolean;
  order?: "oldest" | "newest";
  fold?: "newest" | "full" | number;
  foldLines?: number;
  /**
   * minutes
   */
  group?: number;
  days?: boolean;
  avatar?: "in" | "both" | "none";
  bubble?: "tinted" | "plain" | "filled";
  actions?: {
    id: string;
    label: string;
  }[];
  loadingWords?: string;
  /**
   * the thread's name for a screen reader
   */
  label?: string;
};
export type Memory = {
  spec: ThreadSpec;
  height: number;
  card: import("./busy-card.js").BusyCard | null;
  drawn: boolean;
  pending: ThreadSpec | null;
  wired: boolean;
  /**
   * keys shown whole by the reader
   */
  expanded: Set<string>;
  watcher: ResizeObserver | null;
  /**
   * the width the folds were last measured at
   */
  width: number;
};
/**
 * The options as the page wrote them: the spec first, then the attribute, then the default.
 * @param {HTMLElement} el @param {ThreadSpec} spec
 */
export declare function threadOptions(
  el: HTMLElement,
  spec: ThreadSpec,
): {
  bubble: "tinted" | "plain" | "filled";
  order: "oldest" | "newest";
  fold: number | "full" | "newest";
  foldLines: number;
  group: number;
  days: boolean;
  avatar: "in" | "both" | "none";
};
/**
 * "Monday 13/10/2026".
 * @param {number} ms @param {string} locale
 */
export declare function dayLabel(ms: number, locale: string): string;
/**
 * Split messages (oldest first) into runs that share one header: the same
 * side and sender, the same day, each within `minutes` of the one before.
 * System lines stand alone.
 * @param {readonly ThreadMessage[]} messages @param {number} minutes
 * @returns {ThreadMessage[][]}
 */
export declare function groupMessages(
  messages: readonly ThreadMessage[],
  minutes: number,
): ThreadMessage[][];
/**
 * Draw (or redraw) a thread.
 * @param {HTMLElement} el the root, a `.kp-thread`
 * @param {ThreadSpec} spec
 */
export declare function setThread(el: HTMLElement, spec: ThreadSpec): void;
/**
 * Fill every `[data-kp-thread]` under `root` that carries its spec as JSON in
 * the attribute (a page that draws with setThread() needs nothing).
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachThreads(root?: ParentNode): () => void;
