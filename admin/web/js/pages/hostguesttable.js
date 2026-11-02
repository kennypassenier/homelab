// The Containers card's three views of the same guests, built empty: the
// data table, the entity-card grid and the pressure list.

import { h } from "../dom.js";
import { declareField } from "../drivable.js";

// review M5: every page field Live view may set is declared.
const GUEST_SEARCH = declareField({
  id: "host-guest-search",
  page: "host",
  what: "search the containers by name",
});

/** @returns {{read: HTMLElement, tbody: HTMLElement, table: HTMLElement, cards: HTMLElement, bullets: HTMLElement, views: HTMLElement}} */
export function guestViews() {
  const read = h("span", { class: "hk-muted", role: "status" });
  const tbody = h("tbody", { id: "host-guests" });
  const th = (
    /** @type {string} */ label,
    /** @type {Record<string, string>} */ a = {},
  ) => h("th", { scope: "col", ...a }, label);
  const table = h(
    "div",
    {
      class: "kp-datatable hk-dt",
      id: "host-containers-table",
      "data-kp-datatable": "",
      "data-kp-cards": "",
      "data-kp-expandable": "",
      "data-kp-page-size": "100",
      "data-kp-page-sizes": "none",
      "data-kp-sort-multi": "click",
      "data-kp-remember": "host-containers",
      "data-kp-busy-overlay": "",
      "data-kp-state": "loading",
    },
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("input", {
        class: "kp-datatable__search",
        type: "search",
        id: GUEST_SEARCH,
        "data-kp-datatable-search": "",
        placeholder: "Search the containers   /",
        "aria-label": "Search the containers (press / to jump here)",
      }),
    ),
    // The failed plate is the package's own: fail(reason) draws the reason
    // at heading size with its Retry, filling the table's footprint.
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h("caption", { class: "kp-sr-only" }, "Guests on the host"),
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            th("Guest", { "data-kp-sort": "number" }),
            th("Stack", { "data-kp-sort": "text" }),
            th("State", { "data-kp-sort": "text", "data-kp-filter": "choice" }),
            th("CPU", { "data-kp-sort": "number" }),
            th("Memory", { "data-kp-sort": "number" }),
            th("Lock", { "data-kp-sort": "text" }),
            th("Kind", { "data-kp-sort": "text", "data-kp-filter": "choice" }),
          ),
        ),
        tbody,
      ),
    ),
    h(
      "div",
      { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
      h(
        "div",
        { "data-kp-datatable-empty-none": "" },
        h("p", { class: "kp-empty__title" }, "No guests on this host"),
        h("p", null, "Create a stack and its container shows here."),
      ),
      h(
        "div",
        { "data-kp-datatable-empty-nomatch": "", hidden: "" },
        h("p", { class: "kp-empty__title" }, "No guest matches"),
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--secondary",
            "data-kp-datatable-clear": "",
          },
          "Clear the search and filters",
        ),
      ),
    ),
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("p", {
        class: "kp-datatable__status",
        "data-kp-datatable-status": "",
        role: "status",
        "aria-live": "polite",
      }),
      h("div", { class: "kp-datatable__pager", "data-kp-datatable-pager": "" }),
    ),
  );
  const cards = h("div", {
    class: "kp-entity-grid",
    "data-kp-entity-grid": "",
    "data-kp-entity-link": "/stacks/{key}",
    role: "list",
  });
  const bullets = h("div", { class: "kp-bullets", "data-kp-bullets": "" });
  const views = h("div", { class: "hk-ho-view" }, table, cards, bullets);
  return { read, tbody, table, cards, bullets, views };
}
