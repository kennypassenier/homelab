// DOM helpers shared by the pages: element building, the kp datatable
// block every table uses (ui-tables), the error box and report fetching.

import { routeError } from "./doctor.js";
import {
  replaceTableParams,
  tableFromParams,
  tableToParams,
} from "./urlstate.js";
import { VIEW_EVENT } from "/static/kp/js/datatable.js";

/**
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag
 * @param {Record<string, string> | null} [attrs]
 * @param {...(Node | string)} children
 * @returns {HTMLElementTagNameMap[K]}
 */
export function h(tag, attrs, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (k === "class") e.className = v;
    else e.setAttribute(k, v);
  }
  e.append(...children);
  return e;
}

/**
 * A table cell; a number cell is marked so it lines up.
 * @param {string} text
 * @param {string} [cls]
 */
export function td(text, cls) {
  return h("td", cls ? { class: cls } : null, text);
}

/** @param {{label: string, tone: string}} st */
export function badgeCell(st) {
  return h("td", { class: `state ${st.tone}` }, h("span", null, st.label));
}

/**
 * @typedef {{label: string, sort: string, order?: string, filter?: string,
 *   cls?: string}} Column
 */

/**
 * The kp datatable block every table on the dashboard is (ui-tables):
 * sortable columns, Shift+click for a second key, its sort remembered
 * under its own name, no row selection.
 * @param {{remember: string, caption: string, search: string,
 *   columns: Column[], state?: "loading", pageSize?: number,
 *   pageSizes?: string}} spec pageSize/pageSizes: page a long table
 *   (the logs) instead of showing every row
 */
export function tableBlock(spec) {
  const tbody = h("tbody");
  const head = h(
    "tr",
    null,
    ...spec.columns.map((c) => {
      /** @type {Record<string, string>} */
      const a = { "data-kp-sort": c.sort };
      if (c.order) a["data-kp-sort-order"] = c.order;
      if (c.filter) a["data-kp-filter"] = c.filter;
      if (c.cls) a.class = c.cls;
      return h("th", a, c.label);
    }),
  );
  /** @type {Record<string, string>} */
  const wrapAttrs = {
    class: "kp-datatable",
    "data-kp-datatable": "",
    "data-kp-sort-multi": "",
    "data-kp-remember": spec.remember,
    "data-kp-page-sizes": spec.pageSizes ?? "none",
  };
  if (spec.pageSize) wrapAttrs["data-kp-page-size"] = String(spec.pageSize);
  if (spec.state) wrapAttrs["data-kp-state"] = spec.state;
  const wrap = h(
    "div",
    wrapAttrs,
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("input", {
        class: "kp-datatable__search",
        "data-kp-datatable-search": "",
        type: "search",
        placeholder: spec.search,
        "aria-label": spec.search,
      }),
    ),
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table grid" },
        h("caption", null, spec.caption),
        h("thead", null, head),
        tbody,
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
      ...(spec.pageSize
        ? [
            h("div", {
              class: "kp-datatable__pager",
              "data-kp-datatable-pager": "",
            }),
          ]
        : []),
    ),
  );
  return { wrap, tbody };
}

/**
 * A readable box for a route's `{what, why, fix}`.
 * @param {import("./doctor.js").RouteError} e
 */
export function errorBox(e) {
  return h(
    "div",
    { class: "kp-alert kp-alert--destructive error", role: "alert" },
    h("strong", null, `Could not read ${e.what}`),
    h("p", null, `Why: ${e.why}`),
    ...(e.fix ? [h("p", null, `What to do: ${e.fix}`)] : []),
  );
}

/**
 * Ask a JSON route. The login redirect and an error answer both come back
 * as a RouteError, never as a thrown exception (an abort still throws).
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @returns {Promise<{ok: true, body: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export async function fetchJson(url, what, signal) {
  /** @type {Response} */
  let r;
  try {
    r = await fetch(url, { headers: { accept: "application/json" }, signal });
  } catch (e) {
    if (signal?.aborted) throw e;
    return { ok: false, error: routeError(what, 0, null) };
  }
  if (r.redirected && new URL(r.url).pathname === "/login") {
    location.assign("/login");
    return { ok: false, error: routeError(what, 401, null) };
  }
  /** @type {unknown} */
  let body = null;
  try {
    body = await r.json();
  } catch {
    body = null;
  }
  if (!r.ok || !body || typeof body !== "object")
    return { ok: false, error: routeError(what, r.status, body) };
  return { ok: true, body };
}

/**
 * Ask a report route: the host's JSON answer is under `report`.
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @returns {Promise<{ok: true, report: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export async function fetchReport(url, what, signal) {
  const r = await fetchJson(url, what, signal);
  if (!r.ok) return r;
  return { ok: true, report: r.body.report };
}

/**
 * feat-overview-8: a table's search and filters live in the address. What
 * the address holds is applied once; every change is written back without a
 * new history entry, so Back still leaves the page.
 * @param {import("/static/kp/js/datatable.js").DataTableHandle | null} handle
 * @param {string} name the table's remember name
 * @returns {() => void} stop following
 */
export function bindTableUrl(handle, name) {
  if (!handle) return () => {};
  const wanted = tableFromParams(name, new URLSearchParams(location.search));
  if (wanted.query) handle.query(wanted.query);
  for (const [col, v] of Object.entries(wanted.filters))
    handle.filter(Number(col), v);
  const write = () => {
    const v = handle.view();
    const search = replaceTableParams(
      location.search,
      name,
      tableToParams(name, { query: v.query, filters: v.filters }),
    );
    if (search !== location.search)
      history.replaceState(history.state, "", location.pathname + search);
  };
  handle.element.addEventListener(VIEW_EVENT, write);
  return () => handle.element.removeEventListener(VIEW_EVENT, write);
}

/**
 * Label and value pairs as a definition list.
 * @param {HTMLElement} dl
 * @param {{label: string, value: string}[]} facts
 */
export function fillFacts(dl, facts) {
  dl.replaceChildren(
    ...facts.flatMap((x) => [h("dt", null, x.label), h("dd", null, x.value)]),
  );
}

/**
 * kp-themes progress bars in one group, so the tracks line up.
 * @param {{label: string, pct: number, value: string}[]} bars
 */
export function progressGroup(bars) {
  return h(
    "div",
    { class: "kp-progress-group" },
    ...bars.map((b) => {
      const p = h("progress", {
        class: "kp-progress",
        max: "100",
        value: String(Math.max(0, Math.min(100, b.pct))),
        "aria-label": b.label,
      });
      return h(
        "div",
        { class: "kp-progress__wrap" },
        h("span", { class: "kp-progress__label" }, b.label),
        p,
        h("span", { class: "kp-progress__value" }, b.value),
      );
    }),
  );
}

/**
 * The same kp tab row for every tabbed page: links, so each tab has its own
 * address (feat-overview-8), marked the ARIA way.
 * @param {string} label
 * @param {{href: string, label: string, current: boolean}[]} tabs
 */
export function tabRow(label, tabs) {
  return h(
    "div",
    { class: "kp-tabs__list", role: "tablist", "aria-label": label },
    ...tabs.map((t) =>
      h(
        "a",
        {
          class: "kp-tab",
          role: "tab",
          href: t.href,
          "aria-selected": String(t.current),
          ...(t.current ? { "aria-current": "page" } : {}),
        },
        t.label,
      ),
    ),
  );
}
