// The kp-themes module chassis-rs serves at /static/kp/js/auto-refresh.js (kp-themes 11.1.1),
// copied from kp-themes js/auto-refresh.d.ts: chassis serves the module, not its types.
/** An automatic refresh control. */
export declare const REFRESH = "[data-kp-refresh]";
/** Fired on the control at each refresh the app should do. */
export declare const REFRESH_EVENT = "kp-refresh";
/** Fired when the interval changed. */
export declare const REFRESH_INTERVAL_EVENT = "kp-refresh-interval";
/** The interval a page opens with, in seconds, and the choices offered. */
export declare const REFRESH_DEFAULT = 30;
export declare const REFRESH_CHOICES: number[];
export type RefreshHandle = {
  element: HTMLElement;
  /**
   * seconds, 0 = paused
   */
  interval: () => number;
  /**
   * any whole number of seconds
   */
  setInterval: (seconds: number, by?: "user" | "script") => void;
  /**
   * refresh at once
   */
  now: (reason?: "now" | "retry") => void;
  /**
   * the app's answer to the refresh in progress
   */
  done: (ok: boolean) => void;
};
/** The handle of an attached control. @param {Element} element */
export declare const refreshOf: (element: Element) => RefreshHandle | null;
/**
 * Tell a control the app has finished the refresh it asked for.
 * @param {Element} element @param {boolean} ok
 */
export declare function refreshDone(element: Element, ok: boolean): void;
/**
 * Set the interval from the consumer (any whole number of seconds; 0 pauses).
 * @param {Element} element @param {number} seconds
 */
export declare function setRefreshInterval(
  element: Element,
  seconds: number,
): void;
/**
 * Wire every refresh control under `root`.
 * @param {ParentNode} [root]
 * @param {{ now?: () => number }} [options]
 * @returns {() => void} detach
 */
export declare function attachRefresh(
  root?: ParentNode,
  {
    now,
  }?: {
    now?: () => number;
  },
): () => void;
