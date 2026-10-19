// The kp-themes module chassis-rs serves at /static/kp/js/notices.js (kp-themes 11.0.0),
// copied from kp-themes js/notices.d.ts: chassis serves the module, not its types.
/** A notification centre. */
export declare const NOTICES = "[data-kp-notices]";
export declare const NOTICE_FIX_EVENT = "kp-notice-fix";
export declare const NOTICE_READ_EVENT = "kp-notice-read";
export declare const NOTICES_READ_ALL_EVENT = "kp-notices-read-all";
export declare const NOTICES_SOURCE_EVENT = "kp-notices-source";
export declare const NOTICES_RETRY_EVENT = "kp-notices-retry";
export declare const NOTICES_END_QUIET_EVENT = "kp-notices-end-quiet";
export declare const NOTICES_INBOX_EVENT = "kp-notices-open-inbox";
/** The severities, worst first. */
export declare const NOTICE_SEVERITIES: readonly [
  "critical",
  "warning",
  "info",
];
export type Notice = {
  /**
   * stable id: the same key is the same row on every refresh
   */
  key: string;
  severity: "critical" | "warning" | "info";
  /**
   * what happened, in one line
   */
  title: string;
  /**
   * when, ms since the epoch
   */
  since: number;
  /**
   * what it affects
   */
  consequence?: string;
  /**
   * what to do
   */
  remedy?: string;
  /**
   * the action that fixes it
   */
  fix?: {
    label: string;
  };
  read?: boolean;
  /**
   * the source it came from (a key of `sources`)
   */
  source?: string;
};
export type NoticeSource = {
  key: string;
  label: string;
  muted?: boolean;
};
export type NoticesModel = {
  state?: "ready" | "loading" | "empty" | "error";
  items?: Notice[];
  sources?: NoticeSource[];
  /**
   * ISO or epoch ms
   */
  quietUntil?: number | string | null;
  /**
   * why it could not be read
   */
  error?: string;
};
export type NoticesHandle = {
  element: HTMLElement;
  set: (model: NoticesModel) => void;
  /**
   * the number the counter shows
   */
  count: () => number;
  /**
   * open the panel
   */
  open: () => void;
};
/** The handle of an attached centre. @param {Element} element */
export declare const noticesOf: (element: Element) => NoticesHandle | null;
/**
 * Give a centre its notices.
 * @param {Element} element @param {NoticesModel} model
 */
export declare function setNotices(element: Element, model: NoticesModel): void;
/**
 * Wire every notification centre under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachNotices(root?: ParentNode): () => void;
