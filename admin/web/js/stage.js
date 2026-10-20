// feat-kpadopt-2 (WP-6): the stage a list or a block sits in while it is
// read. `inner` goes inside a kp card that carries the four data states
// (js/card-state.js): its loading is the package's busy card over the whole
// footprint, its empty and failed plates fill the same box. A flat card (no
// frame of its own) so it can sit inside a section; the caller sets the
// state with setCardState and attaches it once with attachCardStates.

import { h } from "./dom.js";

/**
 * @param {HTMLElement} inner
 * @returns {HTMLElement}
 */
export function stage(inner) {
  return h(
    "div",
    { class: "kp-card hb-stage", "data-kp-state": "loading" },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, inner),
    ),
  );
}
