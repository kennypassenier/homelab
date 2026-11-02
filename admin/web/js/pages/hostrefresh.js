// The Host page's one refresh control (feat-kp11demo-11). It replaces the
// three timers: the figures, the containers and the line to the host are
// read together, at the interval you choose; R reads now.

import { h } from "../dom.js";
import { formGrid, gridRow } from "../gridrow.js";
import { REFRESH_S } from "../quiet.js";
import { REFRESH_EVENT } from "/static/kp/dist/kp-themes.min.js";

/** Seconds between two readings of the container list and the ping. */
const EVERY_S = REFRESH_S;

/** @param {(() => Promise<boolean | void>)[]} loaders every read the control triggers; a false answer is a failed read */
export function createRefresh(loaders) {
  // Replaces the three timers: the figures, the containers and the line to
  // the host are read together, at the interval you choose; R reads now.
  const refreshEl = h("div", {
    class: "kp-refresh",
    "data-kp-refresh": String(EVERY_S),
    "data-kp-look": "segmented",
    "data-kp-refresh-choices": "0,15,30,60,300",
    "data-kp-remember": "host-refresh",
    "data-kp-refresh-label": "Refresh",
    "data-kp-stale-after": String(EVERY_S * 3),
  });
  const refreshRead = h(
    "span",
    { class: "hk-muted", role: "status" },
    "The figures, the containers and the line to the host are read together.",
  );
  // The kit's own refresh row: the bar is a direct child of the grid and
  // draws its label in the label track; the sentence sits under it.
  const refreshRow = formGrid(
    [refreshEl, gridRow({ control: refreshRead, span: "control-value" })],
    "hk-refresh",
  );
  refreshEl.addEventListener(REFRESH_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    void Promise.all(loaders.map((read) => read()))
      .then((all) => {
        const ok = all.every(Boolean);
        d.done(ok);
        refreshRead.textContent = ok
          ? "Read the host figures, the containers and the line to the host."
          : "The last read failed; the figures stay as they were.";
      })
      .catch(() => d.done(false));
  });
  return { row: refreshRow, el: refreshEl };
}
