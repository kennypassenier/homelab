// DOM helpers shared by the pages: element building, the kp datatable
// block every table uses (ui-tables), the error box and report fetching.

import { routeError } from "./doctor.js";
import {
  replaceTableParams,
  tableFromParams,
  tableToParams,
} from "./urlstate.js";
import { freshRead } from "./slowread.js";
import { busyWords, emptyWords, failReason } from "./tablestate.js";
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

/**
 * The one shared shape for a top-level section's heading (rule 8, Kenny
 * 2026-10-02: "ik moet niet raden naar wat een functie doet, alles moet
 * duidelijk zijn" — every section says, in one plain sentence, what it
 * does and when it is used, so nobody has to guess from the heading
 * alone). `level` picks `h1` (a page's own title) or `h2` (a section
 * inside a page, e.g. a `<details>` block's `<summary>`); the description
 * always renders as the next sibling paragraph, in one shared class so
 * every page's section intro reads the same.
 * @param {string} title
 * @param {string} description one plain sentence
 * @param {{level?: "h1" | "h2" | "h3"}} [opts]
 */
export function sectionHeader(title, description, opts = {}) {
  return h(
    "hgroup",
    { class: "section-head" },
    h(opts.level ?? "h2", null, title),
    h("p", { class: "section-head__desc measured" }, description),
  );
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
 * width; kp puts the multi-sort summary and its reset on a line of their
 * own under it, so sorting moves nothing; loading shows kp's skeleton on a
 * first load and dims the rows on a refresh, with kp's spinner, the page's
 * own words and kp's counter in the status line (busy()); kp's empty slot
 * says "nothing" and "nothing matches" apart; kp's failed slot carries the
 * reason and a Try again (fail(), `kp-datatable-retry`, which each page
 * already hears). All of it kp-themes 7.3.0 (chassis-rs 2.4.1).
 * @param {{remember: string, caption: string, search: string,
 *   columns: Column[], state?: "loading", pageSize?: number,
 *   pageSizes?: string, select?: {label: string, actions: Node[]},
 *   nothing?: string, busyOverlay?: boolean}} spec
 *   pageSize/pageSizes: page a long table (the logs) instead of showing
 *   every row. select: a checkbox column first and an action bar that
 *   shows while rows are ticked; each row brings its own
 *   `selectCell(key)`. nothing: the empty box's words when the table has
 *   no rows at all. busyOverlay: false leaves out the big loading layer
 *   over the rows (`busyOverlay`; on by default).
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
  // kp-themes' future `data-kp-busy-overlay` (busyOverlay below, a shim).
  if (spec.busyOverlay !== false) wrapAttrs["data-kp-busy-overlay"] = "";
  const status = h("p", {
    class: "kp-datatable__status",
    "data-kp-datatable-status": "",
    role: "status",
    "aria-live": "polite",
  });
  // kp's empty slot, in its two parts: kp shows the one that applies
  // ("nothing yet" or "nothing matches", fix-85); the page words the
  // second from the view (how many rows the search and filters hide).
  const noneTitle = h("p", { class: "kp-empty__title" });
  const matchTitle = h("p", { class: "kp-empty__title" });
  const matchBody = h("p", { class: "kp-empty__body" });
  const empty = h(
    "div",
    { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
    h("div", { "data-kp-datatable-empty-none": "" }, noneTitle),
    h(
      "div",
      { "data-kp-datatable-empty-nomatch": "" },
      matchTitle,
      matchBody,
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
  );
  noneTitle.textContent = spec.nothing ?? "Nothing to show.";
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

  const overlay =
    spec.busyOverlay !== false ? busyOverlay(wrap, status) : () => {};

  // ── Loading: kp's spinner, the page's words and kp's own counter ─────
  /** @type {{text: string, since: number, given: boolean} | null} */
  let busy = null;
  /** @type {number | null} when the rows on screen were read */
  let readAt = null;
  const handle = () => dataTable(wrap);
  /** Hand the words to kp once its handle exists (busy(), fix-80/84). */
  const giveBusy = () => {
    const hd = handle();
    if (!busy || busy.given || !hd) return;
    busy.given = true;
    hd.busy({ text: busy.text, since: busy.since });
  };
  const stopBusy = () => {
    const was = busy;
    busy = null;
    if (was?.given) handle()?.busy(null);
    return was ? (Date.now() - was.since) / 1000 : 0;
  };
  wrap.addEventListener(VIEW_EVENT, (e) => {
    const v = /** @type {CustomEvent} */ (e).detail;
    const words = emptyWords(v, spec.nothing ?? "Nothing to show.");
    matchTitle.textContent = words.title;
    matchBody.textContent = words.body;
    matchBody.hidden = words.body === "";
    // A table attached after loading() began gets its words on its first
    // render (giveBusy renders once more; `given` stops it there).
    giveBusy();
  });

  /**
   * The table is asking for its rows: a first load shows kp's skeleton, a
   * refresh keeps the old rows dimmed and says from when they are; kp's
   * spinner, the page's words and kp's counter in the status line.
   * @param {{words?: string, expect?: number, overlay?: boolean}} [o]
   *   expect: the usual seconds, for the host's slow reads; overlay:
   *   false keeps the big layer off for this load (a page showing its last
   *   answer while it reads again), as kp's busy({ overlay })
   */
  const loading = (o = {}) => {
    stopBusy();
    overlay(o.overlay);
    busy = {
      text: busyWords({
        words: o.words ?? "Asking the host…",
        expect: o.expect ?? null,
        shownFrom:
          readAt != null &&
          tbody.querySelector("tr:not([data-kp-skeleton-row])")
            ? // 24h, not the viewer's locale (Kenny, 2026-10-02): no
              // am/pm, consistent with dd/mm/yyyy HH:MM everywhere else.
              new Date(readAt).toLocaleTimeString(undefined, {
                hour: "2-digit",
                minute: "2-digit",
                hour12: false,
              })
            : null,
      }),
      since: Date.now(),
      given: false,
    };
    const hd = handle();
    if (hd && hd.view().state !== "loading") hd.state("loading");
    giveBusy();
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
   * The read failed: kp's failed slot says what, why and what to do, with
   * its Try again (fail(), fix-85). Returns the seconds the load took.
   * @param {import("./doctor.js").RouteError} e
   */
  const fail = (e) => {
    const secs = stopBusy();
    handle()?.fail(failReason(e));
    return secs;
  };
  /**
   * Changes the empty box's "nothing at all" words after the table was
   * built (fix-179: a per-stack read that found zero rows because every
   * stack failed, rather than because the fleet truly has none, says so
   * instead of the page's fixed `spec.nothing`).
   * @param {string} text
   */
  const setNothing = (text) => {
    noneTitle.textContent = text;
  };
  return { wrap, tbody, loading, ready, failed: fail, setNothing };
}

/**
 * The busy overlay: while the table loads, a large spinner with the busy
 * words and kp's counter over the rows, under the header (Kenny,
 * 2026-09-29: the 1em spinner in the status line under a long table went
 * unseen).
 *
 * A LOCAL SHIM of kp-themes main 05d62af6 (scope-138: busy({ overlay }),
 * the `data-kp-busy-overlay` attribute, the `.kp-datatable__busy-overlay`
 * layer with its `.kp-datatable__busy-panel`, the --kp-busy-overlay-veil and
 * --kp-busy-overlay-spinner knobs), with exactly its names and placement,
 * to delete when the chassis pin carries it. A thin layer on kp's busy():
 * it reads kp's state (`aria-busy` on the wrapper) and mirrors kp's status
 * line, words and counter, and keeps no counter of its own. The layer is
 * aria-hidden; the status line stays the live region. Placed over the
 * skeleton or the dimmed rows, it inserts nothing, so nothing moves.
 * @param {HTMLElement} wrap the `.kp-datatable`
 * @param {HTMLElement} status kp's status line
 * @returns {(on: boolean | undefined) => void} this load's choice, as
 *   kp's busy({ overlay }); undefined is the wrapper's attribute
 */
function busyOverlay(wrap, status) {
  const words = h("span", { class: "kp-datatable__busy-words" });
  const clock = h("span", { class: "kp-datatable__busy-clock" });
  const panel = h(
    "div",
    { class: "kp-datatable__busy-panel" },
    h("span", { class: "kp-spinner" }),
    words,
    clock,
  );
  const layer = h(
    "div",
    { class: "kp-datatable__busy-overlay", "aria-hidden": "true", hidden: "" },
    panel,
  );
  wrap.append(layer);
  /** @type {boolean | undefined} */
  let chosen;
  const wanted = () => chosen ?? wrap.hasAttribute("data-kp-busy-overlay");
  const sync = () => {
    const table = wrap.querySelector("table");
    if (wrap.getAttribute("aria-busy") !== "true" || !wanted() || !table) {
      layer.hidden = true;
      return;
    }
    // kp's status line while loading: its spinner, the words, then its
    // counter in `[data-kp-busy-clock]`.
    const tick = status.querySelector("[data-kp-busy-clock]");
    let text = "";
    status.childNodes.forEach((n) => {
      if (n !== tick && !(n instanceof Element && n.matches(".kp-spinner")))
        text += n.textContent ?? "";
    });
    words.textContent = text.trim();
    clock.textContent = tick?.textContent ?? "";
    clock.hidden = !tick;
    // kp's placement: from the header's bottom to the table box's bottom,
    // within its left and right. A table not laid out (hidden) shows none.
    const host = wrap.getBoundingClientRect();
    if (host.height === 0) {
      layer.hidden = true;
      return;
    }
    const box = (
      wrap.querySelector(".kp-table-wrap") ?? table
    ).getBoundingClientRect();
    const top = (
      table.tHead ??
      table.tBodies[0] ??
      table
    ).getBoundingClientRect()[table.tHead ? "bottom" : "top"];
    const px = (/** @type {number} */ n) => `${Math.max(0, Math.round(n))}px`;
    layer.style.setProperty(
      "--kp-busy-overlay-top",
      px(top - host.top - wrap.clientTop),
    );
    layer.style.setProperty(
      "--kp-busy-overlay-bottom",
      px(host.bottom - Math.max(box.bottom, top) - wrap.clientTop),
    );
    layer.style.setProperty(
      "--kp-busy-overlay-left",
      px(box.left - host.left - wrap.clientLeft),
    );
    layer.style.setProperty(
      "--kp-busy-overlay-right",
      px(host.right - box.right - wrap.clientLeft),
    );
    layer.hidden = false;
  };
  new MutationObserver(sync).observe(status, {
    childList: true,
    subtree: true,
    characterData: true,
  });
  new MutationObserver(sync).observe(wrap, {
    attributes: true,
    attributeFilter: ["aria-busy", "data-kp-busy-overlay", "hidden"],
  });
  if (typeof ResizeObserver === "function")
    new ResizeObserver(sync).observe(wrap);
  sync();
  return (on) => {
    chosen = on;
    sync();
  };
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
  const text = await r.text().catch(() => "");
  /** @type {unknown} */
  let body = null;
  try {
    body = JSON.parse(text);
  } catch {
    body = null;
  }
  if (!r.ok || !body || typeof body !== "object")
    return { ok: false, error: routeError(what, r.status, body, text) };
  return { ok: true, body };
}

/**
 * A slow read (shell/slow.rs): the dashboard runs it once and answers each
 * request within 20 s; a 202 `{running, run}` says it is still on its way,
 * and the page asks again for that run until the answer is there. No
 * request lives long enough for a proxy to cut it (Cloudflare: 100 s).
 * When the dashboard holds an earlier answer, `onLast(body)` gets it at
 * once while one new run reads again (slow-reads, `slowread.js`).
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @param {(body: any) => void} [onLast]
 * @returns {Promise<{ok: true, body: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export function slowRead(url, what, signal, onLast) {
  return freshRead(fetchJson, url, what, signal, onLast);
}

/**
 * `slowRead` for a report route: the host's JSON answer is under `report`.
 * @param {string} url
 * @param {string} what
 * @param {AbortSignal} [signal]
 * @param {(body: any) => void} [onLast] the earlier answer, whole body
 * @returns {Promise<{ok: true, report: any, body: any} | {ok: false, error: import("./doctor.js").RouteError}>}
 */
export async function slowReport(url, what, signal, onLast) {
  const r = await slowRead(url, what, signal, onLast);
  if (!r.ok) return r;
  return { ok: true, report: r.body.report, body: r.body };
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
 * A row of stat tiles from a plain list, redrawn whole (Kenny, 2026-10-01):
 * for a count that does not need its own live-updating node.
 * @param {{label: string, value: string}[]} stats
 */
export function statGrid(stats) {
  return h(
    "div",
    { class: "stat-grid" },
    ...stats.map((s) => statTile(s.label, s.value).el),
  );
}

/**
 * Refills a stat grid in place, the stat-tile counterpart of `fillFacts`.
 * @param {HTMLElement} grid
 * @param {{label: string, value: string}[]} stats
 */
export function fillStatGrid(grid, stats) {
  grid.replaceChildren(...stats.map((s) => statTile(s.label, s.value).el));
}

/**
 * One stat tile with an updatable value node, for a panel that repaints a
 * number in place rather than rebuilding the whole grid (Kenny, 2026-10-01:
 * job panel and batch panel totals).
 * @param {string} label
 * @param {string} [value]
 */
export function statTile(label, value = "") {
  const valueEl = h("span", { class: "stat-tile__value" }, value);
  const el = h(
    "div",
    { class: "stat-tile" },
    h("span", { class: "stat-tile__label" }, label),
    valueEl,
  );
  return { el, value: valueEl };
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
