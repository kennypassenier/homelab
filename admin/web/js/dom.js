// DOM helpers shared by the pages: element building, the kp datatable
// block every table uses (ui-tables), the error box and report fetching.

import { routeError } from "./doctor.js";
import { formatClock } from "./format.js";
import {
  replaceTableParams,
  tableFromParams,
  tableToParams,
} from "./urlstate.js";
import { freshRead } from "./slowread.js";
import { busyWords, emptyWords, failReason } from "./tablestate.js";
import { VIEW_EVENT, dataTable } from "/static/kp/dist/kp-themes.min.js";
import { leave } from "/static/kp/dist/kp-themes.min.js";
import { update } from "/static/kp/dist/kp-themes.min.js";
import { statePlate } from "/static/kp/dist/kp-themes.min.js";
import {
  buildProgressbar,
  setIndeterminate,
  setProgress as kpSetProgress,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * @typedef {Node | string | number | null | undefined | false | Child[]}
 *   Child
 * @typedef {Record<string, string | number | boolean | null | undefined |
 *   ((e: any) => void)>} Attrs
 */

/**
 * The one element factory: `class` sets the class name, an `on…`
 * attribute holding a function becomes a listener, a `null` / `false` /
 * `undefined` attribute is left out and `true` is an empty attribute;
 * children may be nested arrays, numbers or empty (null, false).
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag
 * @param {Attrs | null} [attrs]
 * @param {...Child} kids
 * @returns {HTMLElementTagNameMap[K]}
 */
export function h(tag, attrs, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v == null || v === false) continue;
    if (typeof v === "function") e.addEventListener(k.slice(2), v);
    else if (k === "class") e.className = String(v);
    else e.setAttribute(k, v === true ? "" : String(v));
  }
  /** @param {Child} c */
  const add = (c) => {
    if (c == null || c === false) return;
    if (Array.isArray(c)) c.forEach(add);
    else e.append(typeof c === "number" ? String(c) : c);
  };
  kids.forEach(add);
  return e;
}

/**
 * Keeps every cell of `tbody` labelled with its column's header
 * (`data-label`, which kp-themes' card layout prints before the value on a
 * phone), as rows are added. A cell spanning columns, or one a page
 * labelled itself, is left alone.
 * @param {HTMLElement} tbody
 * @param {() => string[]} labels the header of each column, in order
 */
export function labelCells(tbody, labels) {
  const stamp = () => {
    const names = labels();
    for (const tr of /** @type {HTMLTableRowElement[]} */ ([
      ...tbody.children,
    ])) {
      let at = 0;
      for (const td of /** @type {HTMLTableCellElement[]} */ ([
        ...tr.children,
      ])) {
        const span = Number(td.getAttribute("colspan") ?? 1);
        if (span === 1 && !td.hasAttribute("data-label") && names[at])
          td.setAttribute("data-label", names[at]);
        at += span;
      }
    }
  };
  stamp();
  if (typeof MutationObserver !== "undefined")
    new MutationObserver(stamp).observe(tbody, { childList: true });
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
 * kp-themes' badge variant for a state tone (`ok`, `warn`, `bad`; any
 * other tone is the plain badge).
 * @type {Record<string, string>}
 */
const STATE_BADGE = {
  ok: "success",
  warn: "warning",
  bad: "destructive",
  info: "info",
};

/**
 * A state as kp-themes' badge: the word on its status plate.
 * @param {{label: string, tone: string}} st
 */
export function stateBadge(st) {
  const variant = STATE_BADGE[st.tone];
  return h(
    "span",
    { class: `kp-badge${variant ? ` kp-badge--${variant}` : ""}` },
    st.label,
  );
}

/** @param {{label: string, tone: string}} st */
export function badgeCell(st) {
  return h("td", null, stateBadge(st));
}

/**
 * @typedef {{label: string, sort: string | null, order?: string,
 *   filter?: string, cls?: string}} Column
 *   sort: null for a column that does not sort (its actions, say).
 */

/**
 * The kp datatable block every table on the dashboard is (ui-tables):
 * sortable columns, each header clicked a further key (data-kp-sort-multi="click"), its sort remembered
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
 * @param {{remember: string, caption: string, search: string | null,
 *   columns: Column[], state?: "loading", pageSize?: number,
 *   pageSizes?: string, select?: {label: string, actions: Node[]},
 *   nothing?: string, busyOverlay?: boolean, captionHidden?: boolean}} spec
 *   pageSize/pageSizes: page a long table (the logs) instead of showing
 *   every row. select: a checkbox column first and an action bar that
 *   shows while rows are ticked; each row brings its own
 *   `selectCell(key)`. nothing: the empty box's words when the table has
 *   no rows at all. busyOverlay: false leaves out kp's big loading layer
 *   over the rows (`data-kp-busy-overlay`; on by default). captionHidden: the
 *   caption stays for a screen reader but is not drawn — for a table whose
 *   section heading (`sectionHeader`) already names it right above
 *   (fix-230: the tiny caption repeated the heading under it).
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
      const a = {};
      if (c.sort) a["data-kp-sort"] = c.sort;
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
    // redesign-kit-17: in a narrow box (a phone) each row is a card with
    // its cells labelled, never a table scrolling sideways (kp-themes'
    // card layout; `labelCells` writes the labels it reads).
    "data-kp-cards": "",
    "data-kp-sort-multi": "click",
    "data-kp-remember": spec.remember,
    "data-kp-page-sizes": spec.pageSizes ?? "none",
  };
  if (spec.pageSize) wrapAttrs["data-kp-page-size"] = String(spec.pageSize);
  if (spec.state) wrapAttrs["data-kp-state"] = spec.state;
  // kp-themes' busy overlay (8.1.0; flat in a narrow table since 9.2.0).
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
    // search null: a short table the page filters itself has no box.
    ...(spec.search
      ? [
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
        ]
      : []),
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
        h(
          "caption",
          spec.captionHidden ? { class: "kp-sr-only" } : null,
          spec.caption,
        ),
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

  // ── Loading: kp's spinner, the page's words and kp's own counter ─────
  /** @type {{text: string, since: number, given: boolean, overlay?: boolean} | null} */
  let busy = null;
  /** @type {number | null} when the rows on screen were read */
  let readAt = null;
  const handle = () => dataTable(wrap);
  /** Hand the words to kp once its handle exists (busy(), fix-80/84). */
  const giveBusy = () => {
    const hd = handle();
    if (!busy || busy.given || !hd) return;
    busy.given = true;
    // fix-260: this load's overlay choice reaches kp's busy(); `overlay:
    // false` keeps a page's last answer readable while it reads again.
    hd.busy({
      text: busy.text,
      since: busy.since,
      ...(busy.overlay === undefined ? {} : { overlay: busy.overlay }),
    });
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
    busy = {
      text: busyWords({
        words: o.words ?? "Asking the host…",
        expect: o.expect ?? null,
        shownFrom:
          readAt != null &&
          tbody.querySelector("tr:not([data-kp-skeleton-row])")
            ? // 24h (redesign-final X4: the one clock format).
              formatClock(readAt / 1000)
            : null,
      }),
      since: Date.now(),
      given: false,
      overlay: o.overlay,
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
  labelCells(tbody, () => [
    ...(spec.select ? [""] : []),
    ...spec.columns.map((c) => c.label),
  ]);
  return { wrap, tbody, loading, ready, failed: fail, setNothing };
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
 * A readable failed state for a route's `{what, why, fix}`: the package's
 * failed plate (what could not be read as its title, the why and the fix under
 * it), with a Retry when the caller can read again.
 * @param {import("./doctor.js").RouteError} e
 * @param {() => void} [retry]
 */
export function errorBox(e, retry) {
  const plate = statePlate(document, {
    kind: "error",
    title: `Could not read ${e.what}`,
    text: [`Why: ${e.why}`, ...(e.fix ? [`What to do: ${e.fix}`] : [])].join(
      "\n",
    ),
    ...(retry ? { actionLabel: "Try again", onAction: retry } : {}),
  });
  plate.classList.add("error");
  return plate;
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
 * pointer): kp-themes' `.kp-state-word`, whose `data-kp-words` lists the
 * words; change it with kp's `setStateWord(el, word)`, which also adds a
 * new word to the list. The text a table reads, sorts and filters on stays
 * `text`.
 * @param {string} text
 * @param {string[]} words every text this element can show
 */
export function stateWord(text, words) {
  return h(
    "span",
    {
      class: "kp-state-word",
      "data-kp-words": [text, ...words].join("\n"),
    },
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
 * @param {import("/static/kp/dist/kp-themes.min.js").DataTableHandle | null} handle
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
 * A row of stat tiles from a plain list, redrawn whole (Kenny, 2026-10-01):
 * for a count that does not need its own live-updating node. kp-themes'
 * key figures without a trend: `.kp-kpis` of `.kp-kpi`.
 * @param {{label: string, value: string}[]} stats
 */
export function statGrid(stats) {
  return h(
    "div",
    { class: "kp-kpis", "data-kp-kpis-columns": "all 3 2 1" },
    ...stats.map((s) => statTile(s.label, s.value).el),
  );
}

/**
 * Repaint a live list by its rows' keys, so a row that went away leaves
 * the theme's way (kp-themes' `leave()`) instead of vanishing: a row whose
 * key is still there is swapped for its new paint in place (a repaint,
 * which stays still under `arrive: "new"`), a row with a new key is put
 * where it belongs and arrives, and a row whose key is gone plays its exit
 * where it stood and is then taken out. Children without a key (a
 * skeleton, an empty line) go at once.
 * @param {Element} list
 * @param {Element[]} next the new rows, in order, each carrying a key
 * @param {(el: Element) => string | null} [keyOf]
 * @returns {Promise<void>} settled once every leaving row is gone
 */
export function patchKeyed(
  list,
  next,
  keyOf = (el) => el.getAttribute("data-kp-row-key"),
) {
  // A row that still arrives carries `data-kp-leaving` as well (kp's
  // arrival is its leave played backwards, `data-kp-arriving` beside it):
  // it is a live row, and treating it as leaving left it in place next to
  // its own repaint (two Setup rows in the Inbox, both `#setup-noenv`).
  const leavingNow = (/** @type {Element} */ c) =>
    c.hasAttribute("data-kp-leaving") && !c.hasAttribute("data-kp-arriving");
  /** @type {Map<string, Element>} */
  const old = new Map();
  for (const c of [...list.children]) {
    if (leavingNow(c)) continue;
    const k = keyOf(c);
    if (k == null || old.has(k)) c.remove();
    else old.set(k, c);
  }
  const keep = new Set(next.map(keyOf));
  const gone = [...old].filter(([k]) => !keep.has(k)).map(([, c]) => c);
  const leaving = new Set(gone);
  let cursor = list.firstElementChild;
  const skip = () => {
    while (cursor && (leaving.has(cursor) || leavingNow(cursor)))
      cursor = cursor.nextElementSibling;
  };
  for (const n of next) {
    skip();
    const k = keyOf(n);
    const was = k == null ? undefined : old.get(k);
    if (was && was === cursor) cursor = was.nextElementSibling;
    // The same node kept (a row built once and reused) only moves.
    list.insertBefore(n, cursor);
    if (was && was !== n) was.remove();
  }
  return Promise.all(
    gone.map((el) => leave(/** @type {HTMLElement} */ (el))),
  ).then(() => undefined);
}

/**
 * The attribute that names a value a live repaint may change in place
 * (a state word, a count, a CPU figure), so the change plays the theme's
 * update (kp-themes' `update()`) instead of only swapping the text.
 */
export const LIVE = "data-live";

/**
 * Write a live value in place: a change plays the theme's update; the
 * first fill, or the same text again, is written plainly.
 * @param {Element} el
 * @param {string | number} text
 */
export function setLive(el, text) {
  const s = String(text);
  if (el.textContent === s) return;
  if (!el.isConnected || el.textContent === "") el.textContent = s;
  else void update(el, s);
}

/**
 * One stat tile (`.kp-kpi`: label over a figure) with an updatable value
 * node, for a panel that repaints a number in place rather than rebuilding
 * the whole grid (Kenny, 2026-10-01: job panel and batch panel totals).
 * @param {string} label
 * @param {string} [value]
 */
export function statTile(label, value = "") {
  const valueEl = h("span", { class: "kp-kpi__value" }, value);
  const el = h(
    "div",
    { class: "kp-kpi" },
    h("span", { class: "kp-kpi__label" }, label),
    valueEl,
  );
  return { el, value: valueEl };
}

/**
 * Set a `progressBar`'s share done, 0 to 100, or null for busy with no
 * count to go by (kp-themes' progressbar.js: `aria-valuenow` and the
 * `--kp-value` it paints kept in step, busy as `data-kp-indeterminate`).
 * @param {HTMLElement} el
 * @param {number | null} pct
 */
export function setProgress(el, pct) {
  if (pct == null || !Number.isFinite(pct)) setIndeterminate(el, true);
  else kpSetProgress(el, pct);
}

/**
 * A kp-themes progress bar (`.kp-progressbar`), its track, fill and head
 * written by kp's own `buildProgressbar`.
 * @param {string} label what the bar measures, for a screen reader
 * @param {number | null} [pct] 0 to 100; null: busy
 * @param {string} [cls] further classes
 * @returns {HTMLElement}
 */
export function progressBar(label, pct = 0, cls = "") {
  const el = buildProgressbar(
    h("div", {
      class: cls ? `kp-progressbar ${cls}` : "kp-progressbar",
      "aria-label": label,
      "aria-valuemin": "0",
      "aria-valuemax": "100",
    }),
  );
  setProgress(el, pct);
  return el;
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

/**
 * Load a page's own stylesheet once (the shell's index.html links only
 * app.css). The one loader every page and kit imports (redesign-flows
 * review item 10); a page's sheet is added on its first mount and kept.
 * @param {string} href
 */
export function ensureStyle(href) {
  if (typeof document === "undefined") return;
  if (document.querySelector(`link[data-page-style="${href}"]`)) return;
  const l = document.createElement("link");
  l.rel = "stylesheet";
  l.href = href;
  l.dataset.pageStyle = href;
  document.head.append(l);
}
