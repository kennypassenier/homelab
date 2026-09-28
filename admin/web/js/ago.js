// feat-overview-4: every piece of live data says how old it is, and the
// words tick. An element made by `agoEl` carries its moment in
// `data-ago-at`; one timer for the whole page rewrites them all each second,
// so a page never needs its own.

import { agoText } from "./format.js";

/**
 * A span that reads "measured 12 s ago" and keeps ticking.
 * @param {string} verb "measured", "read", …
 * @param {number | null} [at] unix seconds; set it later with `setAgo`
 * @param {{live?: boolean}} [opts] live: the data should renew itself, so
 *   a reading older than three minutes is marked stale (the host reads
 *   the containers' status every 60 s)
 * @returns {HTMLSpanElement}
 */
export function agoEl(verb, at = null, opts = {}) {
  const e = document.createElement("span");
  e.className = "measured";
  e.dataset.agoVerb = verb;
  if (opts.live) e.dataset.agoLive = "";
  setAgo(e, at);
  return e;
}

/**
 * @param {HTMLElement} e an element from `agoEl`
 * @param {number | null | undefined} at unix seconds
 */
export function setAgo(e, at) {
  if (at == null) delete e.dataset.agoAt;
  else e.dataset.agoAt = String(at);
  paint(e, Date.now() / 1000);
}

/** @param {HTMLElement} e @param {number} now */
function paint(e, now) {
  const at = e.dataset.agoAt == null ? null : Number(e.dataset.agoAt);
  e.textContent = agoText(e.dataset.agoVerb ?? "measured", at, now);
  // Live data that stopped renewing is marked, so a stale page is visible
  // at a glance.
  if (e.dataset.agoLive != null)
    e.dataset.stale = String(at != null && now - at > 180);
}

/** Start the one timer. @returns {() => void} stop */
export function startAgoTicker() {
  const t = setInterval(() => {
    const now = Date.now() / 1000;
    document
      .querySelectorAll("[data-ago-verb]")
      .forEach((e) => paint(/** @type {HTMLElement} */ (e), now));
  }, 1000);
  return () => clearInterval(t);
}
