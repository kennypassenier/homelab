// feat-kp11demo-14 (the slim rail): System's pages stand in a slim rail at
// the side of the page, as icons with their names on hover, instead of a
// second bar; it opens to names with « » (the choice is remembered on this
// device) and, with the pointer resting on it for 150 ms, peeks open over the
// page without moving it. One group, "Sign-in and notices", opens as a panel
// beside the rail. kp-themes' `.kp-sidenav` does all of it (js/sidenav.js);
// this only lists the pages (areas.js, the one list the bar, the palette and
// the System landing also read) and marks the current one. On a phone the
// rail gives way to the tab bar's More sheet.

import { SUB_PAGES, subPageAt } from "./areas.js";
import { h } from "./dom.js";
import { attachSidenavs } from "/static/kp/js/sidenav.js";

/** The icon of each page, drawn with the text colour (24 by 24, one stroke). */
const ICONS = /** @type {Record<string, string[]>} */ ({
  host: ["M4 5h16v6H4z", "M4 13h16v6H4z", "M8 8h.01", "M8 16h.01"],
  metrics: ["M3 17l5-6 4 4 6-8", "M3 21h18"],
  fleetview: ["M12 5v4", "M5 19v-4h14v4", "M12 9H5v6", "M12 9h7v6"],
  firewall: ["M12 3l8 3v6c0 4.5-3.2 7.8-8 9-4.8-1.2-8-4.5-8-9V6z"],
  settings: [
    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
    "M19 12a7 7 0 0 0-.1-1.2l2-1.5-2-3.4-2.3.9a7 7 0 0 0-2-1.2L14.2 3h-4l-.4 2.6a7 7 0 0 0-2 1.2l-2.3-.9-2 3.4 2 1.5a7 7 0 0 0 0 2.4l-2 1.5 2 3.4 2.3-.9a7 7 0 0 0 2 1.2l.4 2.6h4l.4-2.6a7 7 0 0 0 2-1.2l2.3.9 2-3.4-2-1.5c.1-.4.1-.8.1-1.2z",
  ],
  shell: ["M4 6l6 6-6 6", "M13 18h7"],
  group: ["M7 11V8a5 5 0 0 1 10 0v3", "M5 11h14v9H5z"],
});

/** @param {string} id */
function iconOf(id) {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.7");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  for (const d of ICONS[id] ?? ICONS.settings) {
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", d);
    svg.append(p);
  }
  return svg;
}

/** The pages in the rail's list, in order; the rest are the group. */
const MAIN = ["host", "metrics", "fleetview", "firewall", "settings", "shell"];
const GROUP = ["sign-in", "presets", "notifications"];

/**
 * @param {string} id
 * @param {string} current the sub-page id the location is
 * @param {string | null} count an exact count to show, when one is known
 */
function link(id, current, count = null) {
  const p = SUB_PAGES.find((s) => s.id === id);
  if (!p) return null;
  return h(
    "li",
    null,
    h(
      "a",
      {
        class: "kp-sidenav__link",
        href: p.href,
        title: p.what,
        ...(id === current ? { "aria-current": "page" } : {}),
      },
      h(
        "span",
        { class: "kp-sidenav__icon", "aria-hidden": "true" },
        iconOf(id),
      ),
      h("span", { class: "kp-sidenav__label" }, p.label),
      count ? h("span", { class: "kp-sidenav__badge" }, count) : null,
    ),
  );
}

/**
 * The rail's markup for the page the location is.
 * @param {string} current the sub-page id (areas.js `subPageAt`), or ""
 * @returns {HTMLElement}
 */
export function railEl(current) {
  const group = h(
    "li",
    { class: "kp-sidenav__category", "data-kp-sidenav-expanded": "" },
    h(
      "button",
      { type: "button", class: "kp-sidenav__category-toggle" },
      h(
        "span",
        { class: "kp-sidenav__icon", "aria-hidden": "true" },
        iconOf("group"),
      ),
      h("span", { class: "kp-sidenav__label" }, "Sign-in and notices"),
    ),
    h(
      "div",
      { class: "kp-sidenav__submenu" },
      h(
        "ul",
        { class: "kp-sidenav__list" },
        ...GROUP.map((i) => link(i, current)),
      ),
    ),
  );
  return /** @type {HTMLElement} */ (
    h(
      "nav",
      {
        class: "kp-sidenav app-rail",
        id: "system-rail",
        "aria-label": "System pages",
        "data-kp-sidenav-slim": "",
        "data-kp-sidenav-slim-collapsed": "",
        "data-kp-sidenav-expand-on-hover": "",
        "data-kp-sidenav-hover-delay": "150",
        "data-kp-sidenav-remember": "homelab-system-rail",
      },
      h(
        "div",
        { class: "kp-sidenav__header" },
        h(
          "a",
          {
            class: "kp-sidenav__title",
            href: "/system",
            "data-kp-sidenav-slim-hide": "",
          },
          "System",
        ),
        h(
          "span",
          {
            class: "kp-sidenav__title",
            "data-kp-sidenav-slim-show": "",
            "aria-hidden": "true",
          },
          "S",
        ),
      ),
      h(
        "div",
        { class: "kp-sidenav__scroll" },
        h(
          "ul",
          { class: "kp-sidenav__list" },
          ...MAIN.map((i) => link(i, current)),
          h("li", null, h("hr", { class: "kp-sidenav__divider" })),
          group,
        ),
      ),
      h(
        "div",
        { class: "kp-sidenav__footer" },
        h(
          "button",
          {
            type: "button",
            class: "kp-sidenav__link",
            "data-kp-sidenav-slim-toggle": "",
            "aria-controls": "system-rail",
            title: "Open or fold the rail",
          },
          h(
            "span",
            {
              class: "kp-sidenav__icon",
              "aria-hidden": "true",
              "data-kp-sidenav-slim-hide": "",
            },
            "«",
          ),
          h(
            "span",
            {
              class: "kp-sidenav__icon",
              "aria-hidden": "true",
              "data-kp-sidenav-slim-show": "",
            },
            "»",
          ),
        ),
      ),
    )
  );
}

/**
 * Keep the rail in step with the location: drawn on every System page, gone
 * on the others. The slot is `#rail-slot` in index.html.
 * @param {HTMLElement} slot
 * @returns {{sync: (inSystem: boolean, path: string, search: string) => void}}
 */
export function mountSystemRail(slot) {
  /** @type {(() => void) | null} */
  let detach = null;
  let shownFor = "";
  return {
    sync(inSystem, path, search) {
      const body = /** @type {HTMLElement} */ (slot.parentElement);
      if (!inSystem) {
        detach?.();
        detach = null;
        slot.replaceChildren();
        slot.hidden = true;
        body.removeAttribute("data-rail");
        shownFor = "";
        return;
      }
      const clean = path.replace(/\/+$/, "") || "/";
      const current = subPageAt(clean, new URLSearchParams(search))?.id ?? "";
      if (detach && shownFor === current) return;
      // Redrawing keeps the rail's own state (remembered by kp), so only the
      // current mark moves: rebuild just the marks when the rail stands.
      const rail = slot.firstElementChild;
      if (rail && detach) {
        const here = SUB_PAGES.find((x) => x.id === current)?.href;
        for (const a of rail.querySelectorAll("a.kp-sidenav__link"))
          if (a.getAttribute("href") === here)
            a.setAttribute("aria-current", "page");
          else a.removeAttribute("aria-current");
        shownFor = current;
        return;
      }
      slot.replaceChildren(railEl(current));
      slot.hidden = false;
      body.setAttribute("data-rail", "");
      detach = attachSidenavs(slot);
      shownFor = current;
    },
  };
}
