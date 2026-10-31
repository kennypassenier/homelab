// kp11 Activity: a data area (a timeline, a grid) covered by the theme's
// own empty or failed plate, filling the area's whole footprint, so
// Empty and Error stand where the data stood and nothing around them moves
// (UI_PRINCIPLES: loading/empty/failed fill the footprint).

import { h } from "./dom.js";
import { statePlate } from "/static/kp/dist/kp-themes.min.js";

/**
 * Cover `host` (an element with `class="ac-cover"`) with a plate, or take
 * the plate away with `kind` null.
 * @param {HTMLElement} host
 * @param {"empty" | "error" | null} kind
 * @param {{title?: string, text?: string, retry?: () => void}} [words]
 */
export function cover(host, kind, words = {}) {
  host.querySelector(":scope > .ac-plate")?.remove();
  if (!kind) return;
  host.append(
    h(
      "div",
      { class: "ac-plate" },
      statePlate(document, {
        kind,
        title: words.title ?? "",
        ...(words.text ? { text: words.text } : {}),
        ...(words.retry ? { actionLabel: "Retry", onAction: words.retry } : {}),
      }),
    ),
  );
}
