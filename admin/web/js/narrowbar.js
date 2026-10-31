// kp11 Activity: the filter bar above a timeline (kp toolbar): one line that
// says how many rows a pick leaves, and one removable tag for every pick (a
// range dragged on the timeline, a day or a stack pressed in the grid) with
// Clear all. The tags and the line are the page's; the bar is kp's.

import { h } from "./dom.js";
import { setToolbarActive } from "/static/kp/dist/kp-themes.min.js";

/**
 * @param {{label: string, markRemove: (button: HTMLElement, id: string) => void,
 *   markClear: (button: HTMLElement) => void}} spec
 *   label: the bar's accessible name; markRemove / markClear: the page marks
 *   a tag's cross (a row per tag id) and Clear all for Live view, with its
 *   own declared controls
 * @returns {{el: HTMLElement, state: HTMLElement,
 *   set: (tags: {id: string, label: string}[]) => void}}
 */
export function narrowBar(spec) {
  const state = h("div", { class: "kp-toolbar__state", role: "status" });
  const el = h(
    "div",
    {
      class: "kp-toolbar ac-bar",
      "data-kp-toolbar": "",
      role: "group",
      "aria-label": spec.label,
    },
    state,
    h("div", { class: "kp-toolbar__active", hidden: "" }),
  );
  return {
    el,
    state,
    set: (tags) => {
      // kp-themes 11.2.0's decorate hook marks the cross of each tag and
      // Clear all for Live view as the toolbar builds them.
      setToolbarActive(el, tags, {
        decorate: (part, info) => {
          if (info.kind === "tag-remove")
            spec.markRemove(part, info.value ?? "");
          else spec.markClear(part);
        },
      });
    },
  };
}
