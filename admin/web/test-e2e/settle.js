// ux-small-1 (gate of 5.1.4, 2026-11-06): waiting for a page's real settled
// state instead of a fixed time. A control measured while the kit's 560 ms
// size motion plays reads anything from 1 to 44 px (a whole-screen run reported
// a different "defect" each time), so a page is read only when what it asked
// for has arrived and nothing around a control is still changing its size.

/** How long nothing may be in flight or moving before a page counts as settled (ms). */
const QUIET_MS = 300;
/** How often the page is looked at while it settles (ms). */
const POLL_MS = 100;
/** A page that is not settled after this long fails by name, never read half-drawn (ms). */
const SETTLE_MS = 10000;

/**
 * Keep count of a page's requests. An EventSource never ends: it is the live
 * stream every page holds open, not something the page waits for.
 * @param {import("playwright").Page} page
 * @returns {Set<import("playwright").Request>} the requests in flight
 */
export function trackRequests(page) {
  const inFlight = new Set();
  page.on("request", (r) => {
    if (r.resourceType() !== "eventsource") inFlight.add(r);
  });
  page.on("requestfinished", (r) => inFlight.delete(r));
  page.on("requestfailed", (r) => inFlight.delete(r));
  return inFlight;
}

/**
 * In the page: what still keeps it from being read, and the controls under 44 px,
 * taken in one go so no animation can start between the two.
 * A part below the fold stays `aria-busy` until it is scrolled to (a chart
 * asks for its data then), and one in a closed panel has no box: neither is
 * on its way, so only a busy part on screen counts.
 * @param {{selector: string}} arg
 */
function pageState({ selector }) {
  const SIZE =
    /width|height|size|margin|padding|transform|scale|translate|rotate|inset/i;
  const controls = [...document.querySelectorAll(selector)];
  const moving = document.getAnimations().filter((a) => {
    const fx = /** @type {KeyframeEffect | null} */ (a.effect);
    if (a.playState !== "running" || !fx?.target) return false;
    const resizes = fx
      .getKeyframes()
      .some((k) => Object.keys(k).some((p) => SIZE.test(p)));
    return resizes && controls.some((c) => fx.target?.contains(c));
  });
  const busy = [
    ...document.querySelectorAll(
      '#page [aria-busy="true"], #page [data-kp-state="loading"], #page [data-kp-resizing], #page [data-kp-action-measuring]',
    ),
  ].filter((e) => {
    const b = e.getBoundingClientRect();
    return b.width > 0 && b.bottom > 0 && b.top < innerHeight;
  });
  return {
    drawn: controls.length,
    moving: moving.length,
    busy: busy.length,
    small: controls
      .filter((e) => {
        const b = e.getBoundingClientRect();
        const s = getComputedStyle(e);
        return (
          b.width > 0 &&
          s.visibility !== "hidden" &&
          !e.closest("[hidden], .kp-sr-only") &&
          (b.height < 43.5 || b.width < 43.5)
        );
      })
      .map((e) => {
        const b = e.getBoundingClientRect();
        return `${e.className} ${e.textContent?.trim().slice(0, 20)} (${b.width.toFixed(1)}x${b.height.toFixed(1)})`;
      }),
  };
}

/**
 * Wait until the page holds still, then answer the controls under 44 px on it.
 * Throws, naming what it still waits for, when the page does not settle.
 * @param {import("playwright").Page} page
 * @param {Set<import("playwright").Request>} inFlight from `trackRequests`
 * @param {string} selector the controls to read
 * @returns {Promise<string[]>}
 */
export async function smallControlsOnceSettled(page, inFlight, selector) {
  const deadline = Date.now() + SETTLE_MS;
  /** @type {number | null} */
  let quietSince = null;
  for (;;) {
    const s = await page.evaluate(pageState, { selector });
    const still = s.drawn > 0 && !s.busy && !s.moving && inFlight.size === 0;
    quietSince = still ? (quietSince ?? Date.now()) : null;
    if (quietSince !== null && Date.now() - quietSince >= QUIET_MS)
      return s.small;
    if (Date.now() > deadline)
      throw new Error(
        `${new URL(page.url()).pathname} did not settle in ${SETTLE_MS} ms: ${s.drawn} controls, ${s.busy} busy on screen, ${s.moving} resizing, ${inFlight.size} requests in flight`,
      );
    await page.waitForTimeout(POLL_MS);
  }
}
