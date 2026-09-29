// DOM helpers shared by the pages: element building, the kp datatable
// block every table uses (ui-tables), the error box and report fetching.

import { routeError } from "./doctor.js";
import {
  replaceTableParams,
  tableFromParams,
  tableToParams,
} from "./urlstate.js";
import { busyWords, emptyWords } from "./tablestate.js";
import { VIEW_EVENT, dataTable } from "/static/kp/js/datatable.js";

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
 * under its own name, no row selection (the one exception, the fleet's
 * batch actions, passes `select`).
 *
 * Every kp datatable feature a table that asks for its rows can use
 * (Kenny, 2026-09-29: "Als we componenten gebruiken, dan moeten we alle
 * toepasselijke features ook gebruiken"): the search takes the toolbar's
 * width; the sort summary and kp's reset sit on their own line under it,
 * so sorting moves nothing; loading shows kp's skeleton on a first load and
 * dims the rows on a refresh, with a spinner, the page's own words and a
 * counter in the status line; kp's empty slot says "nothing" and "nothing
 * matches" apart; kp's failed slot carries the reason and a retry
 * (`kp-datatable-retry`, which each page already hears).
 * @param {{remember: string, caption: string, search: string,
 *   columns: Column[], state?: "loading", pageSize?: number,
 *   pageSizes?: string, select?: {label: string, actions: Node[]},
 *   nothing?: string}} spec
 *   pageSize/pageSizes: page a long table (the logs) instead of showing
 *   every row. select: a checkbox column first and an action bar that
 *   shows while rows are ticked; each row brings its own
 *   `selectCell(key)`. nothing: the empty box's words when the table has
 *   no rows at all.
 */
export function tableBlock(spec) {
  const tbody = h("tbody");
  const head = h(
    "tr",
    null,
    ...(spec.select
      ? [
          h(
            "th",
            { class: "select-col" },
            h("input", {
              class: "kp-field__check",
              type: "checkbox",
              "data-kp-select-all": "",
              "aria-label": spec.select.label,
            }),
          ),
        ]
      : []),
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
  const status = h("p", {
    class: "kp-datatable__status",
    "data-kp-datatable-status": "",
    role: "status",
    "aria-live": "polite",
  });
  // The multi-sort summary, given to kp so it does not put its own in the
  // search's row, where every key added shortened the search box.
  const sortSummary = h("p", {
    class: "kp-datatable__status kp-datatable__sort-summary",
    "data-kp-datatable-sort-summary": "",
    "aria-live": "polite",
  });
  const sortReset = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost kp-button--sm",
      "data-kp-datatable-sort-reset": "",
      "data-off": "",
    },
    "Clear sort",
  );
  const failTitle = h("p", { class: "kp-alert__label" });
  const failWhy = h("p");
  const failFix = h("p");
  const failed = h(
    "div",
    {
      class: "kp-alert kp-alert--destructive table-failed",
      "data-kp-datatable-failed": "",
      role: "alert",
      hidden: "",
    },
    failTitle,
    failWhy,
    failFix,
    h(
      "button",
      { type: "button", class: "kp-button", "data-kp-datatable-retry": "" },
      "Try again",
    ),
  );
  const emptyTitle = h("p", { class: "kp-empty__title" });
  const emptyBody = h("p", { class: "kp-empty__body" });
  const emptyClear = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--secondary",
      "data-kp-datatable-clear": "",
    },
    "Clear the search and filters",
  );
  const empty = h(
    "div",
    { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
    emptyTitle,
    emptyBody,
    emptyClear,
  );
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
      { class: "kp-datatable__bar table-sortline" },
      sortSummary,
      sortReset,
    ),
    ...(spec.select
      ? [
          h(
            "div",
            {
              class: "kp-datatable__actions",
              "data-kp-datatable-actions": "",
              hidden: "",
            },
            h("span", { "data-kp-datatable-selected-count": "" }),
            ...spec.select.actions,
          ),
        ]
      : []),
    failed,
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
    empty,
    h(
      "div",
      { class: "kp-datatable__bar" },
      status,
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

  // ── Loading: kp's spinner, the page's words and a counter ────────────
  /** @type {{words: string, expect: number | null, shownFrom: string | null, began: number} | null} */
  let busy = null;
  /** @type {ReturnType<typeof setInterval> | null} */
  let tick = null;
  /** @type {number | null} when the rows on screen were read */
  let readAt = null;
  const handle = () => dataTable(wrap);
  /**
   * SHIM for kp-themes' busy(text) (kp main 8c2c7f8a, fix-80; not in the
   * 7.2.0 chassis vendors): the handle's own busy() when it has one;
   * otherwise exactly what it does, painted after each kp render: a
   * spinner, then the words, in the status line while the table loads.
   * Remove when chassis vendors a kp-themes with busy().
   * @param {string | null} text
   */
  const setBusyText = (text) => {
    const hd = /** @type {any} */ (handle());
    if (hd && typeof hd.busy === "function") {
      hd.busy(text);
      return;
    }
    if (text == null || wrap.dataset.kpState !== "loading") return;
    status.replaceChildren(
      h("span", { class: "kp-spinner", "aria-hidden": "true" }),
      " ",
      text,
    );
  };
  const paintBusy = () => {
    if (!busy) return;
    const w = busyWords({
      words: busy.words,
      seconds: (Date.now() - busy.began) / 1000,
      expect: busy.expect,
      shownFrom: busy.shownFrom,
    });
    setBusyText(`${w.text} ${w.counter}`);
  };
  const stopBusy = () => {
    if (tick) clearInterval(tick);
    tick = null;
    const was = busy;
    busy = null;
    if (was) setBusyText(null);
    return was ? (Date.now() - was.began) / 1000 : 0;
  };
  wrap.addEventListener(VIEW_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail;
    // The sort line: the full summary on hover, the reset only when sorted
    // (its place kept, so the line never changes height or shifts).
    sortSummary.title = sortSummary.textContent ?? "";
    sortReset.toggleAttribute("data-off", (v?.sorts ?? []).length === 0);
    const words = emptyWords(v, spec.nothing ?? "Nothing to show.");
    emptyTitle.textContent = words.title;
    emptyBody.textContent = words.body;
    emptyBody.hidden = words.body === "";
    emptyClear.hidden = !words.clear;
    paintBusy();
  });

  /**
   * The table is asking for its rows: a first load keeps kp's skeleton, a
   * refresh keeps the old rows dimmed and says from when they are.
   * @param {{words?: string, expect?: number}} [o] expect: the usual
   *   seconds, for the host's slow reads
   */
  const loading = (o = {}) => {
    stopBusy();
    busy = {
      words: o.words ?? "Asking the host…",
      expect: o.expect ?? null,
      shownFrom:
        readAt != null && tbody.querySelector("tr:not([data-kp-skeleton-row])")
          ? new Date(readAt).toLocaleTimeString(undefined, {
              hour: "2-digit",
              minute: "2-digit",
            })
          : null,
      began: Date.now(),
    };
    tick = setInterval(paintBusy, 1000);
    const hd = handle();
    if (hd) {
      if (hd.view().state === "loading") paintBusy();
      else hd.state("loading");
    }
  };
  /**
   * The rows are in: kp re-reads them (unless `refresh` is false: a live
   * update that changed rows in place). Returns the seconds the load took.
   * @param {{refresh?: boolean}} [o]
   */
  const ready = (o = {}) => {
    const secs = stopBusy();
    readAt = Date.now();
    const hd = handle();
    if (o.refresh !== false) hd?.refresh();
    if (hd && hd.view().state !== "ready") hd.state("ready");
    return secs;
  };
  /**
   * The read failed: kp's failed slot says why, what to do and offers a
   * retry. Returns the seconds the load took.
   * @param {import("./doctor.js").RouteError} e
   */
  const fail = (e) => {
    const secs = stopBusy();
    failTitle.textContent = `Could not read ${e.what}`;
    failWhy.textContent = `Why: ${e.why}`;
    failFix.textContent = e.fix ? `What to do: ${e.fix}` : "";
    failFix.hidden = !e.fix;
    handle()?.state("failed");
    return secs;
  };
  return { wrap, tbody, loading, ready, failed: fail };
}

/**
 * The checkbox cell of a selectable row (see `tableBlock`'s `select`).
 * @param {string} key the row's key, also written on the row
 * @param {string} label
 */
export function selectCell(key, label) {
  return h(
    "td",
    { class: "select-col" },
    h("input", {
      class: "kp-field__check",
      type: "checkbox",
      "data-kp-select-row": "",
      "aria-label": label,
      value: key,
    }),
  );
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
 * A slow read (shell/slow.rs): the dashboard runs it once and answers each
 * request within 20 s; a 202 `{running, run}` says it is still on its way,
 * and the page asks again for that run until the answer is there. No
 * request lives long enough for a proxy to cut it (Cloudflare: 100 s).
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @returns {Promise<{ok: true, body: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export async function slowRead(url, what, signal) {
  /** @type {number | null} */
  let run = null;
  for (;;) {
    const u =
      run == null
        ? url
        : `${url}${url.includes("?") ? "&" : "?"}run=${encodeURIComponent(run)}`;
    const r = await fetchJson(u, what, signal);
    if (!r.ok || r.body.running !== true) return r;
    run = Number(r.body.run);
  }
}

/**
 * `slowRead` for a report route: the host's JSON answer is under `report`.
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @returns {Promise<{ok: true, report: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export async function slowReport(url, what, signal) {
  const r = await slowRead(url, what, signal);
  if (!r.ok) return r;
  return { ok: true, report: r.body.report };
}

/**
 * Call `paint(seconds)` now and every second until the returned stop is
 * called: the "N s so far" of a read the page waits for.
 * @param {(seconds: number) => void} paint
 * @returns {() => number} stop; hands back the seconds it ran
 */
export function elapsed(paint) {
  const began = Date.now();
  const secs = () => Math.round((Date.now() - began) / 1000);
  paint(0);
  const t = setInterval(() => paint(secs()), 1000);
  return () => {
    clearInterval(t);
    return (Date.now() - began) / 1000;
  };
}

/**
 * A word that changes with a control's state ("on" / "muted"), in a box as
 * wide as the widest word it can show, so nothing beside or after it moves
 * when it changes (Kenny, 2026-09-29: the layout never shifts under the
 * pointer). The other words are only a sizer (CSS `.state-word`); the text
 * a table reads, sorts and filters on stays `text`.
 * @param {string} text
 * @param {string[]} words every text this element can show
 */
export function stateWord(text, words) {
  return h(
    "span",
    { class: "state-word", "data-size": [text, ...words].join("\n") },
    text,
  );
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
