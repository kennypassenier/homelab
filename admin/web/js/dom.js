// DOM helpers shared by the pages: element building, the kp datatable
// block every table uses (ui-tables), the error box and report fetching.

import { routeError } from "./doctor.js";

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
 *   columns: Column[], state?: "loading"}} spec
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
    "data-kp-page-sizes": "none",
  };
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
 * Ask a report route. The login redirect and a 502 both come back as a
 * RouteError, never as a thrown exception.
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @returns {Promise<{ok: true, report: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export async function fetchReport(url, what, signal) {
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
  return { ok: true, report: /** @type {any} */ (body).report };
}
