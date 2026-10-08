// feat-overview-4: every piece of live data says how old it is, and the
// words tick. kp-themes' freshness line (js/freshness.js) does the work:
// one timer for the whole page, the absolute moment in the title, the
// width the words can reach reserved so a unit change moves nothing, and
// the theme's stale plate. This keeps the dashboard's two calls (a verb,
// unix seconds, and live data marked stale after three minutes).

import { attachAgo, setAgo as kpSetAgo } from "/static/kp/js/freshness.js";

/** Live data older than this is stale: the host reads every 60 s. */
const STALE_AFTER_S = 180;

/**
 * A line that reads "measured 12 s ago" and keeps ticking.
 * @param {string} verb "measured", "read", …
 * @param {number | null} [at] unix seconds; set it later with `setAgo`
 * @param {{live?: boolean}} [opts] live: the data should renew itself, so
 *   a reading older than three minutes is marked stale
 * @returns {HTMLElement}
 */
export function agoEl(verb, at = null, opts = {}) {
  const e = document.createElement("time");
  e.className = "measured kp-ago";
  e.setAttribute("data-kp-ago", "");
  e.setAttribute("data-kp-ago-verb", verb);
  if (opts.live) e.setAttribute("data-kp-stale-after", String(STALE_AFTER_S));
  setAgo(e, at);
  return e;
}

/**
 * @param {HTMLElement} e an element from `agoEl`
 * @param {number | null | undefined} at unix seconds
 */
export function setAgo(e, at) {
  kpSetAgo(e, at == null ? null : at * 1000);
}

/** Start the one timer, for every line on the page now and later. @returns {() => void} stop */
export function startAgoTicker() {
  return attachAgo(document);
}
