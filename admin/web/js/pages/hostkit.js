// redesign-host (3.71.0): small LOCAL stand-ins for the shared building
// blocks the redesign foundation puts in `admin/web/js/ui.js` (pageHeader,
// kpi / kpiStrip, attentionBand, section, liveStatus), named and shaped
// like them so the Host page swaps to the shared ones by changing one
// import. Until then they draw the approved Host demo exactly
// (~/.local/share/homelab/redesign-3.71/host.html), styled by
// `css/pages/host.css` under the `hk-` prefix so they never collide with
// the foundation's `nx-` classes.
//
// What the shared versions need for the swap (reported to the foundation):
// pageHeader's `meta` row (chips beside the live status), kpi's `meter`
// (with a tick mark), section's `foot` line and `foldTools` (a collapsible
// card with tools in its head), and the plain-click `toggleGroup` that
// keeps the segmented look.

import { agoEl, setAgo } from "../ago.js";

/**
 * @typedef {Node | string | number | null | undefined | false | Child[]}
 *   Child
 */

/**
 * `dom.js`'s `h`, plus what a page with handlers needs: `on…` attributes
 * become listeners, `null`/`false` attributes are left out, and children
 * may be nested arrays or empty.
 * @param {string} tag
 * @param {Record<string, any> | null} [attrs]
 * @param {...Child} kids
 * @returns {HTMLElement}
 */
export function el(tag, attrs, ...kids) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v == null || v === false) continue;
    if (k === "class") e.className = v;
    else if (k.startsWith("on") && typeof v === "function")
      e.addEventListener(k.slice(2), v);
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

/** Load a page's own stylesheet once (the shell's index.html links only
 * app.css). @param {string} href */
export function ensureStyle(href) {
  if (document.querySelector(`link[data-page-style="${href}"]`)) return;
  const l = document.createElement("link");
  l.rel = "stylesheet";
  l.href = href;
  l.dataset.pageStyle = href;
  document.head.append(l);
}

/**
 * The live freshness of a page or a card, ticking (ago.js).
 * @param {string} [verb]
 * @returns {{el: HTMLElement, set: (at: number | null) => void}}
 */
export function liveStatus(verb = "updated") {
  const ago = agoEl(verb, null, { live: true });
  const e = el("span", { class: "hk-live", role: "status" }, ago);
  return { el: e, set: (at) => setAgo(ago, at) };
}

/**
 * The page header as the demo draws it: title and its one sentence on the
 * left with a meta row (live chips) under them, the actions against the
 * right edge — secondary ones first, the ONE primary last, then the
 * overflow menu. The description is the title's next sibling paragraph
 * (invariants 32, 36, 40, 41).
 * @param {{title: string, sub?: string, desc: string, meta?: Child[],
 *   live?: string | boolean, actions?: Node[], primary?: Node | null,
 *   more?: Node | null}} spec
 */
export function pageHeader(spec) {
  const title = /** @type {HTMLHeadingElement} */ (
    el(
      "h1",
      null,
      spec.title,
      spec.sub ? el("span", { class: "hk-head__sub" }, ` ${spec.sub}`) : null,
    )
  );
  const desc = /** @type {HTMLParagraphElement} */ (
    el("p", { class: "hk-head__desc section-head__desc" }, spec.desc)
  );
  const live =
    spec.live === undefined || spec.live === false
      ? null
      : liveStatus(typeof spec.live === "string" ? spec.live : "updated");
  const meta = el("div", { class: "hk-head__meta" }, spec.meta ?? [], live?.el);
  // `actions-row`: the class the whole-screen label check (fix-236) reads.
  const actions = el(
    "div",
    { class: "hk-head__actions actions-row" },
    spec.actions ?? [],
    spec.primary ?? null,
    spec.more ?? null,
  );
  const e = el("header", { class: "hk-head" }, title, desc, meta, actions);
  return { el: e, title, desc, actions, meta, live };
}

/**
 * @typedef {{key?: string, label: string, value?: string, unit?: string,
 *   ctx?: string, title?: string, tone?: "" | "warn" | "bad",
 *   meter?: {pct: number | null, mark?: number | null, tone?: string}}} Kpi
 */

/**
 * One KPI tile: label, value + unit, context, a meter in the last row
 * (rule 6: the same four rows whatever the value). `set` repaints in place.
 * @param {Kpi} k
 * @returns {{el: HTMLElement, set: (k: Partial<Kpi>) => void}}
 */
export function kpi(k) {
  const label = el("span", { class: "hk-kpi__label" });
  const value = el("span", { class: "hk-kpi__value" });
  const ctx = el("span", { class: "hk-kpi__ctx" });
  const meter = el("div", { class: "hk-meter", "aria-hidden": "true" });
  const e = el("div", { class: "hk-kpi" }, label, value, ctx, meter);
  /** @type {Kpi} */
  let cur = { ...k };
  /** @param {Partial<Kpi>} next */
  const set = (next) => {
    cur = { ...cur, ...next };
    label.textContent = cur.label;
    value.replaceChildren(
      cur.value ?? "—",
      ...(cur.unit ? [el("small", null, cur.unit)] : []),
    );
    ctx.textContent = cur.ctx ?? "";
    if (cur.title) e.title = cur.title;
    e.dataset.key = cur.key ?? cur.label;
    const m = cur.meter;
    meter.className = `hk-meter${m?.tone ? ` hk-meter--${m.tone}` : ""}`;
    const fill = el("i");
    fill.style.width = `${Math.max(0, Math.min(100, m?.pct ?? 0))}%`;
    const mark =
      m?.mark != null
        ? el("b", { title: "promised to stacks" })
        : /** @type {HTMLElement | null} */ (null);
    if (mark) mark.style.left = `${Math.min(100, m?.mark ?? 0)}%`;
    meter.replaceChildren(fill, ...(mark ? [mark] : []));
  };
  set({});
  return { el: e, set };
}

/**
 * A row of KPI tiles, one grid track each.
 * @param {Kpi[]} tiles
 * @param {{label?: string}} [opts]
 */
export function kpiStrip(tiles, opts = {}) {
  /** @type {Map<string, ReturnType<typeof kpi>>} */
  const map = new Map();
  const e = el("section", {
    class: "hk-kpis",
    "aria-label": opts.label ?? "Key figures",
  });
  e.style.setProperty("--kpi-n", String(Math.max(1, tiles.length)));
  for (const t of tiles) {
    const k = kpi(t);
    map.set(t.key ?? t.label, k);
    e.append(k.el);
  }
  return { el: e, tiles: map };
}

/**
 * @typedef {{key?: string, tone: "warn" | "bad" | "info", title: string,
 *   text?: string, action?: Node | null}} Attention
 */

/**
 * The attention band: one alert per problem, worst first, each with its
 * fix; hidden (zero height) when nothing needs a person.
 * @param {Attention[]} [items]
 */
export function attentionBand(items = []) {
  const e = el("div", {
    class: "hk-attention",
    role: "region",
    "aria-label": "Needs a look",
  });
  const order = { bad: 0, warn: 1, info: 2 };
  /** @param {Attention[]} list */
  const set = (list) => {
    const sorted = [...list].sort((a, b) => order[a.tone] - order[b.tone]);
    e.replaceChildren(
      ...sorted.map((a) =>
        el(
          "div",
          {
            class: `kp-alert kp-alert--${a.tone === "bad" ? "destructive" : a.tone === "warn" ? "warning" : "info"} hk-attention__item`,
            role: a.tone === "bad" ? "alert" : "status",
            "data-key": a.key ?? null,
          },
          el(
            "div",
            null,
            el("strong", null, a.title),
            a.text ? el("p", null, a.text) : null,
          ),
          a.action ? el("div", { class: "hk-attention__act" }, a.action) : null,
        ),
      ),
    );
    e.hidden = sorted.length === 0;
  };
  set(items);
  return { el: e, set };
}

/**
 * A card: heading + one sentence (rule 8) left, tools right, the body,
 * and an optional footer line (source left, "read 4 s ago" right).
 * `collapsible` makes it a `<details>` whose summary carries the chevron.
 * @param {{title: string, desc: string, id?: string, tools?: Node[],
 *   foot?: Child[], collapsible?: boolean, open?: boolean,
 *   level?: "h2" | "h3"}} spec
 */
export function section(spec) {
  const desc = el("p", { class: "section-head__desc" }, spec.desc);
  const tools = el(
    "div",
    { class: "hk-card__tools" },
    spec.tools ?? [],
    spec.collapsible
      ? el("span", { class: "hk-chev", "aria-hidden": "true" }, "›")
      : null,
  );
  const head = el(
    "div",
    { class: "hk-card__h" },
    el(spec.level ?? "h2", null, spec.title),
    desc,
    tools,
  );
  const body = el("div", { class: "hk-card__b" });
  const foot = spec.foot
    ? el(
        "div",
        { class: "hk-card__f" },
        spec.foot.map((x) => el("span", null, x)),
      )
    : null;
  const e = spec.collapsible
    ? el(
        "details",
        {
          class: "kp-card hk-card hk-card--fold",
          id: spec.id ?? null,
          open: spec.open ? true : null,
        },
        el("summary", null, head),
        body,
        foot,
      )
    : el(
        "section",
        {
          class: "kp-card hk-card",
          id: spec.id ?? null,
          "aria-label": spec.title,
        },
        head,
        body,
        foot,
      );
  return {
    el: e,
    body,
    foot,
    stop: () => {},
    open: () => {
      if (e instanceof HTMLDetailsElement) e.open = true;
    },
  };
}

/**
 * Plain-click toggle chips in the segmented look (DESIGN_LANGUAGE §10,
 * §12): each click turns one value on or off, several may be on, and
 * "All" (shown pressed when none is) turns them all off again.
 * @param {{label: string, all: {label: string, hint?: string},
 *   chips: {value: string, label: string, hint?: string}[],
 *   onChange: (on: Set<string>) => void,
 *   mark?: (b: HTMLElement, value: string) => void}} spec
 */
export function toggleGroup(spec) {
  /** @type {Set<string>} */
  const on = new Set();
  /** @type {Map<string, HTMLElement>} */
  const counts = new Map();
  const countEl = (/** @type {string} */ v) => {
    const c = el("span", { class: "hk-seg__count" });
    counts.set(v, c);
    return c;
  };
  const allBtn = el(
    "button",
    {
      type: "button",
      "data-v": "all",
      title: spec.all.hint ?? "Show every row",
      onclick: () => {
        on.clear();
        paint();
        spec.onChange(new Set(on));
      },
    },
    spec.all.label,
    countEl("all"),
  );
  const btns = spec.chips.map((c) =>
    el(
      "button",
      {
        type: "button",
        "data-v": c.value,
        title:
          c.hint ??
          "Click to show or hide these; each click turns one on or off",
        onclick: () => {
          if (on.has(c.value)) on.delete(c.value);
          else on.add(c.value);
          // Every value on is the same as none: back to All.
          if (on.size === spec.chips.length) on.clear();
          paint();
          spec.onChange(new Set(on));
        },
      },
      c.label,
      countEl(c.value),
    ),
  );
  spec.mark?.(allBtn, "all");
  btns.forEach((b, i) => spec.mark?.(b, spec.chips[i].value));
  const e = el(
    "div",
    { class: "hk-seg", role: "group", "aria-label": spec.label },
    allBtn,
    btns,
  );
  const paint = () => {
    allBtn.setAttribute("aria-pressed", String(on.size === 0));
    btns.forEach((b, i) =>
      b.setAttribute("aria-pressed", String(on.has(spec.chips[i].value))),
    );
  };
  paint();
  return {
    el: e,
    /** @param {Record<string, number>} n */
    counts: (n) => {
      for (const [v, c] of counts)
        c.textContent = n[v] == null ? "" : String(n[v]);
    },
  };
}

/**
 * A one-of segmented switch ("Changed" / "All").
 * @param {{label: string, items: {value: string, label: string,
 *   hint?: string}[], value: string, onChange: (v: string) => void,
 *   mark?: (b: HTMLElement, value: string) => void}} spec
 */
export function segSwitch(spec) {
  /** @type {Map<string, HTMLElement>} */
  const counts = new Map();
  let cur = spec.value;
  const btns = spec.items.map((it) => {
    const c = el("span", { class: "hk-seg__count" });
    counts.set(it.value, c);
    const b = el(
      "button",
      {
        type: "button",
        "data-v": it.value,
        title: it.hint ?? null,
        onclick: () => {
          cur = it.value;
          paint();
          spec.onChange(cur);
        },
      },
      it.label,
      c,
    );
    spec.mark?.(b, it.value);
    return b;
  });
  const paint = () =>
    btns.forEach((b, i) =>
      b.setAttribute("aria-pressed", String(spec.items[i].value === cur)),
    );
  paint();
  return {
    el: el(
      "div",
      { class: "hk-seg", role: "group", "aria-label": spec.label },
      btns,
    ),
    /** @param {Record<string, number>} n */
    counts: (n) => {
      for (const [v, c] of counts)
        c.textContent = n[v] == null ? "" : String(n[v]);
    },
  };
}

/**
 * Click a header to sort ascending, again descending, a third time back to
 * the original order; Enter/Space do the same; `aria-sort` says which. A
 * cell's `data-sort` overrides its text; numbers sort as numbers.
 * @param {HTMLTableElement} table
 */
export function sortable(table) {
  const head = table.tHead?.rows[0];
  if (!head) return;
  const ths = [...head.cells];
  ths.forEach((th, ci) => {
    if (!th.textContent?.trim()) return;
    th.classList.add("hk-sortable");
    th.tabIndex = 0;
    th.title =
      "Sort by this column; click again to reverse, a third time for the original order";
    const go = () => {
      const now = th.getAttribute("aria-sort");
      const dir =
        now === "ascending"
          ? "descending"
          : now === "descending"
            ? null
            : "ascending";
      ths.forEach((x) => x.removeAttribute("aria-sort"));
      const body = table.tBodies[0];
      const rows = [...body.rows];
      rows.forEach((r, i) => {
        if (r.dataset.order == null) r.dataset.order = String(i);
      });
      if (!dir) {
        rows.sort((a, b) => Number(a.dataset.order) - Number(b.dataset.order));
        body.append(...rows);
        return;
      }
      th.setAttribute("aria-sort", dir);
      /** @param {HTMLTableRowElement} r */
      const key = (r) => {
        const c = r.cells[ci];
        const v = c?.dataset.sort ?? c?.textContent?.trim() ?? "";
        const n = Number(v);
        return v !== "" && Number.isFinite(n) ? n : v.toLowerCase();
      };
      rows.sort((a, b) => {
        const A = key(a);
        const B = key(b);
        return (A > B ? 1 : A < B ? -1 : 0) * (dir === "ascending" ? 1 : -1);
      });
      body.append(...rows);
    };
    th.addEventListener("click", go);
    th.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        go();
      }
    });
  });
}

/**
 * The header's overflow `···` menu: a button and a small list of links,
 * closed by Escape, a click outside or picking an item.
 * @param {{label: string, items: {label: string, hint: string,
 *   href: string, download?: string}[],
 *   mark?: (b: HTMLElement) => void}} spec
 * @returns {{el: HTMLElement, stop: () => void}}
 */
export function moreMenu(spec) {
  const list = el(
    "div",
    { class: "hk-menu", role: "menu", hidden: true },
    spec.items.map((it) =>
      el(
        "a",
        {
          role: "menuitem",
          href: it.href,
          download: it.download ?? null,
          onclick: () => close(),
        },
        el("b", null, it.label),
        el("span", null, it.hint),
      ),
    ),
  );
  const btn = el(
    "button",
    {
      type: "button",
      class: "hk-icon-btn",
      "aria-label": spec.label,
      "aria-haspopup": "menu",
      "aria-expanded": "false",
      title: spec.label,
      onclick: () => (list.hidden ? open() : close()),
    },
    "···",
  );
  spec.mark?.(btn);
  const open = () => {
    list.hidden = false;
    btn.setAttribute("aria-expanded", "true");
  };
  const close = () => {
    list.hidden = true;
    btn.setAttribute("aria-expanded", "false");
  };
  const wrap = el("div", { class: "hk-more" }, btn, list);
  /** @param {MouseEvent} e */
  const outside = (e) => {
    if (!wrap.contains(/** @type {Node} */ (e.target))) close();
  };
  /** @param {KeyboardEvent} e */
  const esc = (e) => {
    if (e.key === "Escape" && !list.hidden) close();
  };
  document.addEventListener("click", outside);
  document.addEventListener("keydown", esc);
  return {
    el: wrap,
    stop: () => {
      document.removeEventListener("click", outside);
      document.removeEventListener("keydown", esc);
    },
  };
}

/**
 * The compact key row under a page ("/ search containers · …").
 * @param {[string, string][]} pairs
 */
export const keyRow = (pairs) =>
  el(
    "p",
    { class: "hk-keys", "aria-label": "Keyboard shortcuts" },
    pairs.map(([k, w]) =>
      el("span", null, el("kbd", { class: "hk-kbd" }, k), w),
    ),
  );
