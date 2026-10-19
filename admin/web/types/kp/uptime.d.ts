/** A strip. */
export declare const UPTIME = "[data-kp-uptime]";
/** Fired on the strip when a cell is pressed: `{ key, index, period }`. */
export declare const UPTIME_PICK_EVENT = "kp-uptime-pick";
/** Fired on the strip, cancelable, before a period's quick info opens: `{ key, index, period, panel }`. */
export declare const UPTIME_OPEN_EVENT = "kp-uptime-open";
/** The states of a period, in the order of how bad they are. */
export declare const UPTIME_STATES: readonly ["up", "degraded", "down", "none"];
/** How many cells a strip waits with when the page has not said (`data-kp-uptime-count`). */
export declare const UPTIME_COUNT = 90;
export type UptimePeriod = {
  /**
   * stable id of the period (an ISO day or hour)
   */
  key: string;
  /**
   * its name as a person reads it ("14/10/2026", "14:00")
   */
  label: string;
  state: "up" | "degraded" | "down" | "none";
  /**
   * the share of the period the service was up, 0 to 1, when measured
   */
  uptime?: number;
  /**
   * how many incidents began in it
   */
  incidents?: number;
  /**
   * the hover card's text; else the label, the state and the figures
   */
  tip?: string;
  /**
   * the period's own page: the cell is a link to it (or, with `open: 'info'`, its quick info links it)
   */
  href?: string;
  /**
   * lines for the period's quick info (what happened, how long), with `open: 'info'`
   */
  details?: string[];
};
export type UptimeSpec = {
  /**
   * oldest first
   */
  periods?: UptimePeriod[];
  /**
   * default `ready` (or `empty` without periods)
   */
  state?: "ready" | "loading" | "empty" | "error";
  /**
   * the strip's title (the service); else `data-kp-uptime-label`
   */
  label?: string;
  /**
   * the axis's start word when every period is shown ("90 days ago"); else the first period's label
   */
  from?: string;
  /**
   * the axis's end word ("Today"); else the latest period's label
   */
  to?: string;
  /**
   * how many cells a loading or empty strip waits with; else `data-kp-uptime-count`, else 90
   */
  count?: number;
  /**
   * the error state's sentence; else the dictionary's
   */
  error?: string;
  /**
   * a press opens the period's quick info; else `data-kp-uptime-open`
   */
  open?: "info";
};
export type UptimeFigures = {
  /**
   * periods with data
   */
  measured: number;
  /**
   * the window's share up, 0 to 1; null without data
   */
  share: number | null;
  incidents: number;
  counts: Record<"up" | "degraded" | "down" | "none", number>;
};
/**
 * @typedef {object} UptimePeriod
 * @property {string} key       stable id of the period (an ISO day or hour)
 * @property {string} label     its name as a person reads it ("14/10/2026", "14:00")
 * @property {'up' | 'degraded' | 'down' | 'none'} state
 * @property {number} [uptime]  the share of the period the service was up, 0 to 1, when measured
 * @property {number} [incidents] how many incidents began in it
 * @property {string} [tip]     the hover card's text; else the label, the state and the figures
 * @property {string} [href]    the period's own page: the cell is a link to it (or, with `open: 'info'`, its quick info links it)
 * @property {string[]} [details] lines for the period's quick info (what happened, how long), with `open: 'info'`
 */
/**
 * @typedef {object} UptimeSpec
 * @property {UptimePeriod[]} [periods] oldest first
 * @property {'ready' | 'loading' | 'empty' | 'error'} [state] default `ready` (or `empty` without periods)
 * @property {string} [label]   the strip's title (the service); else `data-kp-uptime-label`
 * @property {string} [from]    the axis's start word when every period is shown ("90 days ago"); else the first period's label
 * @property {string} [to]      the axis's end word ("Today"); else the latest period's label
 * @property {number} [count]   how many cells a loading or empty strip waits with; else `data-kp-uptime-count`, else 90
 * @property {string} [error]   the error state's sentence; else the dictionary's
 * @property {'info'} [open]    a press opens the period's quick info; else `data-kp-uptime-open`
 */
/**
 * @typedef {object} UptimeFigures
 * @property {number} measured   periods with data
 * @property {number | null} share  the window's share up, 0 to 1; null without data
 * @property {number} incidents
 * @property {Record<'up' | 'degraded' | 'down' | 'none', number>} counts
 */
/**
 * The window's figures from its periods.
 * @param {readonly UptimePeriod[]} periods
 * @returns {UptimeFigures}
 */
export declare function uptimeFigures(
  periods: readonly UptimePeriod[],
): UptimeFigures;
/**
 * A share as a percentage with one decimal, cut rather than rounded, so a window with any time down never reads 100.
 * @param {number} share 0 to 1
 * @returns {string} "99.4", "99.0", "100", "0"
 */
export declare function uptimePercent(share: number): string;
/**
 * How many of `count` cells fit in `width` px, each at least `min` px with `gap` px between.
 * @param {number} count @param {number} width @param {number} min @param {number} gap
 * @returns {number}
 */
export declare function uptimeFit(
  count: number,
  width: number,
  min: number,
  gap: number,
): number;
/**
 * Draw a strip, or let its busy card leave first when it stops loading.
 * @param {HTMLElement} el
 * @param {UptimeSpec} spec
 */
export declare function setUptime(el: HTMLElement, spec: UptimeSpec): void;
/**
 * Wire every strip under `root`: draw the waiting state of a strip not yet given its periods, follow its width, and
 * take its keys and presses.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachUptimes(root?: ParentNode): () => void;
