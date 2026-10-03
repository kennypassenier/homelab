// feat-shell-1 (redesign 3.71.0, FLOWS.md §1 "System"): the rarely needed
// area's landing — the host, metrics, the map, the firewall, the settings,
// presets, notification rules, sign-in and the console, one card each,
// drawn from areas.js's list so this page and the palette never disagree.
// Its problems surface in the Inbox on their own, so a person who never
// opens System misses nothing.

import { SUB_PAGES } from "../areas.js";
import { h } from "../dom.js";
import { pageHeader, section } from "../ui.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const head = pageHeader({
    title: "System",
    desc: "The host, its metrics, the map of the fleet and the settings; rarely needed, since anything wrong here also shows up in the Inbox.",
  });
  const grid = h(
    "nav",
    { class: "nx-actions system-links", "aria-label": "System pages" },
    ...SUB_PAGES.filter((s) => s.area === "system").map((s) =>
      h(
        "a",
        { class: "nx-action", href: s.href },
        h("strong", null, s.label),
        h("span", null, s.what),
      ),
    ),
  );
  const pagesCard = section({
    title: "Everything under System",
    desc: "One card per page: open the one you need; each says what it is for.",
  });
  pagesCard.body.append(grid);
  root.replaceChildren(head.el, pagesCard.el);
  return () => pagesCard.stop();
}
