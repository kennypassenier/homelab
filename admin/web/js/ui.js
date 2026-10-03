// feat-shell-5 (redesign 3.71.0, DESIGN_LANGUAGE.md §1-§12, Kenny approved
// 2026-10-03): the shared building blocks every redesigned page is made
// of, so the pages read as one product. kp-themes components underneath
// (kp-card, kp-alert, kp-badge, kp-breadcrumb, kp-skeleton, kp-empty,
// kp-button) and only kp-themes tokens in app.css's `nx-` layout classes;
// no second design system.
//
//   pageHeader   title, one-sentence description, breadcrumbs, live
//                status ("updated 4 s ago"), one primary action slot
//   kpiStrip     3-6 KPI tiles: label, value, context line, sparkline,
//                each tile a link to its filtered detail
//   attention    one kp-alert per problem, worst first, each with its fix;
//                zero height when all is well
//   toolbar      §12: search · labelled filter groups · view & state,
//                plus the active-filter row; toggleChips is the plain-click
//                on/off chip group (several allowed, Esc / Show all resets)
//   section      a card: heading + one sentence + tools, optionally
//                collapsible and mounted only when first opened
//   skeleton…    loading in the final geometry from the first frame
//   emptyState   why it is empty and what fills it, plus that action
//   stackMark    the per-stack identity mark (hue + mirrored 5×5 pattern
//                from the name), the same in every table and header
//   sparkline    a 28 px trend line for a tile or a table row
//
// The shared time chart lives in timechart.js.

import { agoEl, setAgo } from "./ago.js";
import { h } from "./dom.js";

/** @typedef {{label: string, href?: string}} Crumb */

/**
 * The trail above a page title: `Stacks / gateway / Logs`. Every crumb but
 * the last is a link up (FLOWS.md §1.2).
 * @param {Crumb[]} trail
 * @returns {HTMLElement}
 */
export function breadcrumbs(trail) {
  return h(
    "nav",
    { class: "kp-breadcrumb nx-crumbs", "aria-label": "You are here" },
    h(
      "ol",
      null,
      ...trail.map((c, i) =>
        h(
          "li",
          null,
          c.href && i < trail.length - 1
            ? h("a", { href: c.href }, c.label)
            : h(
                "span",
                i === trail.length - 1 ? { "aria-current": "page" } : null,
                c.label,
              ),
        ),
      ),
    ),
  );
}

/**
 * The live freshness of a page or a card: a dot and "updated 4 s ago",
 * ticking (ago.js), greyed out once it is older than three minutes.
 * @param {string} [verb] "updated", "read", "measured"
 * @returns {{el: HTMLElement, set: (at: number | null) => void}}
 */
export function liveStatus(verb = "updated") {
  const ago = agoEl(verb, null, { live: true });
  const el = h("span", { class: "nx-live", role: "status" }, ago);
  return { el, set: (at) => setAgo(ago, at) };
}

/**
 * @typedef {{title: string, desc: string, crumbs?: Crumb[],
 *   live?: string | boolean, actions?: Node[], primary?: Node | null}}
 *   HeaderSpec
 *   `live`: show a live status (true, or the verb: "updated", "read");
 *   `actions`: at most two secondary actions (and an overflow menu);
 *   `primary`: the ONE primary action, last on the right.
 */

/**
 * The page header (DESIGN_LANGUAGE §1.1): breadcrumbs, then one title row
 * (title left; live status and the actions grouped against the right
 * edge, the primary one last), then the page's one-sentence description
 * right under it. Markup the whole-screen invariants read: `.title-row`
 * holding the `h1`, the description its next sibling `p` (rows 32, 36, 40,
 * 41).
 * @param {HeaderSpec} spec
 * @returns {{el: HTMLElement, title: HTMLHeadingElement,
 *   desc: HTMLParagraphElement, actions: HTMLElement,
 *   live: ReturnType<typeof liveStatus> | null}}
 */
export function pageHeader(spec) {
  const title = h("h1", null, spec.title);
  const live =
    spec.live === undefined || spec.live === false
      ? null
      : liveStatus(typeof spec.live === "string" ? spec.live : "updated");
  const actions = h(
    "div",
    { class: "actions-row nx-head-actions" },
    ...(spec.actions ?? []),
    ...(spec.primary ? [spec.primary] : []),
  );
  const row = h("div", { class: "title-row" }, title);
  if (live || actions.childElementCount > 0) {
    const right = h("div", { class: "nx-head-right" });
    if (live) right.append(live.el);
    right.append(actions);
    row.append(right);
  }
  const desc = h("p", { class: "section-head__desc nx-head-desc" }, spec.desc);
  const el = h(
    "header",
    { class: "nx-head" },
    ...(spec.crumbs?.length ? [breadcrumbs(spec.crumbs)] : []),
    row,
    desc,
  );
  return { el, title, desc, actions, live };
}

/**
 * @typedef {{key?: string, label: string, value?: string, unit?: string,
 *   ctx?: string, tone?: "ok" | "warn" | "bad" | null, href?: string,
 *   spark?: number[], title?: string}} Kpi
 */

/**
 * One KPI tile (DESIGN_LANGUAGE §1.2): label (xs uppercase), value
 * (tabular), a context line, an optional sparkline; a tile with an `href`
 * is a link to its filtered detail. `set` repaints it in place, so the
 * strip never reflows (rule 6).
 * @param {Kpi} k
 * @returns {{el: HTMLElement, set: (k: Partial<Kpi>) => void}}
 */
export function kpi(k) {
  const label = h("span", { class: "nx-kpi__label" });
  const value = h("span", { class: "nx-kpi__value" });
  const ctx = h("span", { class: "nx-kpi__ctx" });
  const spark = h("span", { class: "nx-kpi__spark", "aria-hidden": "true" });
  const el = h(
    k.href ? "a" : "div",
    { class: "nx-kpi", ...(k.href ? { href: k.href } : {}) },
    label,
    value,
    ctx,
    spark,
  );
  /** @type {Kpi} */
  let cur = { ...k };
  const set = (/** @type {Partial<Kpi>} */ next) => {
    cur = { ...cur, ...next };
    label.textContent = cur.label;
    value.replaceChildren(
      cur.value ?? "—",
      ...(cur.unit ? [h("small", null, cur.unit)] : []),
    );
    ctx.textContent = cur.ctx ?? "";
    el.dataset.tone = cur.tone ?? "";
    if (cur.title) el.title = cur.title;
    spark.replaceChildren(
      ...(cur.spark && cur.spark.length > 1 ? [sparkline(cur.spark)] : []),
    );
    if (cur.href && el instanceof HTMLAnchorElement) el.href = cur.href;
  };
  set({});
  return { el, set };
}

/**
 * A row of 3 to 6 KPI tiles; while `loading` each tile is its own skeleton
 * in the final geometry.
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean}} [opts]
 * @returns {{el: HTMLElement, tiles: Map<string, ReturnType<typeof kpi>>}}
 */
export function kpiStrip(tiles, opts = {}) {
  /** @type {Map<string, ReturnType<typeof kpi>>} */
  const map = new Map();
  const el = h("div", {
    class: "nx-kpis",
    role: "group",
    "aria-label": "Key figures",
  });
  el.style.setProperty("--kpi-n", String(Math.max(1, tiles.length)));
  for (const t of tiles) {
    const k = kpi(opts.loading ? { ...t, value: "" } : t);
    if (opts.loading) k.el.dataset.loading = "";
    map.set(t.key ?? t.label, k);
    el.append(k.el);
  }
  return { el, tiles: map };
}

/**
 * @typedef {{key?: string, tone: "warn" | "bad" | "info", title: string,
 *   text?: string, action?: Node | null}} Attention
 */

/**
 * The attention band (DESIGN_LANGUAGE §1.3): one kp-alert per problem,
 * worst first, each with its own fix; absent — zero height, never "all
 * fine" filler — when nothing needs a person.
 * @param {Attention[]} [items]
 * @returns {{el: HTMLElement, set: (items: Attention[]) => void}}
 */
export function attentionBand(items = []) {
  const el = h("div", {
    class: "nx-attention",
    role: "region",
    "aria-label": "Needs a look",
  });
  const order = { bad: 0, warn: 1, info: 2 };
  const set = (/** @type {Attention[]} */ list) => {
    const sorted = [...list].sort((a, b) => order[a.tone] - order[b.tone]);
    el.replaceChildren(
      ...sorted.map((a) =>
        h(
          "div",
          {
            class: `kp-alert kp-alert--${a.tone === "bad" ? "destructive" : a.tone === "warn" ? "warning" : "info"} nx-attention__item`,
            role: a.tone === "bad" ? "alert" : "status",
            ...(a.key ? { "data-key": a.key } : {}),
          },
          h(
            "span",
            { class: "nx-attention__icon", "aria-hidden": "true" },
            a.tone === "info" ? "i" : "!",
          ),
          h(
            "div",
            { class: "nx-attention__text" },
            h("strong", null, a.title),
            ...(a.text ? [h("span", null, a.text)] : []),
          ),
          h(
            "div",
            { class: "nx-attention__act" },
            ...(a.action ? [a.action] : []),
          ),
        ),
      ),
    );
    el.hidden = sorted.length === 0;
  };
  set(items);
  return { el, set };
}

/**
 * @typedef {{value: string, label: string, count?: number | null,
 *   hint?: string}} Chip
 */

/**
 * A group of toggle chips (DESIGN_LANGUAGE §10, §12; Kenny 2026-10-03: "of
 * meerdere kunnen aanklikken"): a plain click turns one value on or off,
 * several may be on, no modifier keys; none on means everything is shown.
 * `reset()` (Show all, Esc) turns them all off again.
 * @param {{label: string, chips: Chip[], selected?: Iterable<string>,
 *   onChange: (selected: Set<string>) => void}} spec
 * @returns {{el: HTMLElement, selected: () => Set<string>,
 *   set: (values: Iterable<string>) => void, reset: () => void,
 *   setChips: (chips: Chip[]) => void}}
 */
export function toggleChips(spec) {
  /** @type {Set<string>} */
  const sel = new Set(spec.selected ?? []);
  const values = h("span", { class: "nx-chips" });
  const el = h(
    "div",
    { class: "nx-tb__group", role: "group", "aria-label": spec.label },
    h("b", null, spec.label),
    values,
  );
  /** @type {Chip[]} */
  let chips = spec.chips;
  const paint = () => {
    values.replaceChildren(
      ...chips.map((c) => {
        const b = h(
          "button",
          {
            type: "button",
            class: "nx-chip-toggle",
            "aria-pressed": String(sel.has(c.value)),
            "data-value": c.value,
            title:
              c.hint ??
              `Show or hide ${c.label}; each click turns it on or off`,
          },
          c.label,
          ...(c.count != null
            ? [h("span", { class: "nx-chip-toggle__count" }, String(c.count))]
            : []),
        );
        b.addEventListener("click", () => {
          if (sel.has(c.value)) sel.delete(c.value);
          else sel.add(c.value);
          paint();
          spec.onChange(new Set(sel));
        });
        return b;
      }),
    );
  };
  paint();
  return {
    el,
    selected: () => new Set(sel),
    set: (v) => {
      sel.clear();
      for (const x of v) sel.add(x);
      paint();
    },
    reset: () => {
      if (sel.size === 0) return;
      sel.clear();
      paint();
      spec.onChange(new Set(sel));
    },
    setChips: (next) => {
      chips = next;
      for (const v of [...sel])
        if (!chips.some((c) => c.value === v)) sel.delete(v);
      paint();
    },
  };
}

/**
 * The toolbar of a list, a table, a log or a chart (DESIGN_LANGUAGE §12):
 * three zones on one grid row inside the card it filters — the search at
 * its own width with its `/` hint inside the box, the labelled filter
 * groups in the middle, view & state (the count, follow, range, Table /
 * Cards) against the right edge — and under it the active-filter row
 * (one removable chip per filter not visible in the bar, plus Clear all),
 * absent when nothing is active. Esc in the search clears it.
 * @param {{search?: {placeholder: string, label?: string, value?: string,
 *   onInput: (q: string) => void}, groups?: HTMLElement[],
 *   state?: Node[]}} spec
 * @returns {{el: HTMLElement, search: HTMLInputElement | null,
 *   state: HTMLElement, setActive: (chips: {label: string,
 *   clear: () => void}[], clearAll?: () => void) => void}}
 */
export function toolbar(spec) {
  /** @type {HTMLInputElement | null} */
  let search = null;
  const zones = [];
  if (spec.search) {
    const s = spec.search;
    search = /** @type {HTMLInputElement} */ (
      h("input", {
        type: "search",
        class: "kp-field__input nx-search",
        placeholder: s.placeholder,
        "aria-label": s.label ?? s.placeholder,
        "aria-keyshortcuts": "/",
      })
    );
    search.value = s.value ?? "";
    const input = search;
    input.addEventListener("input", () => s.onInput(input.value));
    input.addEventListener("keydown", (e) => {
      if (e.key === "Escape" && input.value) {
        e.stopPropagation();
        input.value = "";
        s.onInput("");
      }
    });
    zones.push(
      h(
        "div",
        { class: "nx-tb__search" },
        search,
        h("kbd", { class: "nx-kbd", "aria-hidden": "true" }, "/"),
      ),
    );
  }
  zones.push(h("div", { class: "nx-tb__filters" }, ...(spec.groups ?? [])));
  const state = h("div", { class: "nx-tb__state" }, ...(spec.state ?? []));
  zones.push(state);
  const active = h("div", { class: "nx-tb__active", hidden: "" });
  const el = h(
    "div",
    {
      class: `nx-tb${spec.search ? "" : " nx-tb--nosearch"}`,
      role: "toolbar",
    },
    ...zones,
    active,
  );
  return {
    el,
    search,
    state,
    setActive: (chips, clearAll) => {
      active.replaceChildren(
        h("span", { class: "nx-tb__active-label" }, "Showing only"),
        ...chips.map((c) => {
          const x = h(
            "button",
            {
              type: "button",
              class: "kp-tag nx-tb__chip",
              title: `Remove this filter: ${c.label}`,
            },
            c.label,
            h("span", { "aria-hidden": "true" }, " ✕"),
          );
          x.addEventListener("click", c.clear);
          return x;
        }),
        ...(clearAll
          ? [
              (() => {
                const b = h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--ghost kp-button--sm",
                  },
                  "Clear all",
                );
                b.addEventListener("click", clearAll);
                return b;
              })(),
            ]
          : []),
      );
      active.hidden = chips.length === 0;
    },
  };
}

/**
 * A section of a page as a kp card (DESIGN_LANGUAGE §5): a header row with
 * its title and one sentence (rule 8) on the left and its tools on the
 * right, then the body. `collapsible` makes it a `<details>` (rarely used
 * or destructive things go last, folded); `mount` fills the body, the
 * first time it is open — so a folded section never pays for its read —
 * and its cleanup runs with `stop()`.
 * @param {{title: string, desc: string, id?: string, tools?: Node[],
 *   collapsible?: boolean, open?: boolean,
 *   mount?: (body: HTMLElement) => (() => void) | void,
 *   level?: "h2" | "h3"}} spec
 * @returns {{el: HTMLElement, body: HTMLElement, stop: () => void,
 *   open: () => void}}
 */
export function section(spec) {
  const head = h(
    "div",
    { class: "nx-card__head" },
    h(spec.level ?? "h2", null, spec.title),
    h("p", { class: "section-head__desc" }, spec.desc),
    ...(spec.tools?.length
      ? [h("div", { class: "nx-card__tools" }, ...spec.tools)]
      : []),
  );
  const body = h("div", { class: "nx-card__body" });
  /** @type {(() => void) | null} */
  let cleanup = null;
  let mounted = false;
  const fill = () => {
    if (mounted || !spec.mount) return;
    mounted = true;
    cleanup = spec.mount(body) ?? null;
  };
  /** @type {HTMLElement} */
  let el;
  if (spec.collapsible) {
    const d = /** @type {HTMLDetailsElement} */ (
      h(
        "details",
        {
          class: "kp-card nx-card nx-card--fold",
          ...(spec.id ? { id: spec.id } : {}),
          ...(spec.open ? { open: "" } : {}),
        },
        h("summary", null, head),
        body,
      )
    );
    d.addEventListener("toggle", () => {
      if (d.open) fill();
    });
    if (spec.open) fill();
    el = d;
  } else {
    el = h(
      "section",
      { class: "kp-card nx-card", ...(spec.id ? { id: spec.id } : {}) },
      head,
      body,
    );
    fill();
  }
  return {
    el,
    body,
    stop: () => cleanup?.(),
    open: () => {
      if (el instanceof HTMLDetailsElement) el.open = true;
      fill();
    },
  };
}

/**
 * Lines of skeleton text in the final geometry (DESIGN_LANGUAGE §7):
 * shown from the first frame, replaced in place by the content.
 * @param {number} [lines]
 * @param {string} [label] what is loading, for a screen reader
 */
export function skeletonLines(lines = 3, label = "Loading") {
  return h(
    "div",
    {
      class: "nx-skeleton",
      role: "status",
      "aria-label": label,
      "data-kp-state": "loading",
    },
    ...Array.from({ length: lines }, (_, i) =>
      h("span", {
        class: "kp-skeleton",
        style: `inline-size: ${i === lines - 1 ? 60 : 100}%`,
      }),
    ),
  );
}

/**
 * A table's skeleton: `rows` rows of `cols` cells, the table's own grid.
 * @param {number} rows
 * @param {number} cols
 * @param {string} [label]
 */
export function skeletonTable(rows, cols, label = "Loading") {
  return h(
    "div",
    {
      class: "nx-skeleton nx-skeleton--table",
      role: "status",
      "data-kp-state": "loading",
      "aria-label": label,
      style: `--cols: ${cols}`,
    },
    ...Array.from({ length: rows * cols }, () =>
      h("span", { class: "kp-skeleton" }),
    ),
  );
}

/**
 * A block-sized skeleton (a chart, a calendar) of a fixed height.
 * @param {string} height a CSS length, the final content's height
 * @param {string} [label]
 */
export const skeletonBlock = (height, label = "Loading") =>
  h("div", {
    class: "kp-skeleton kp-skeleton--block nx-skeleton-block",
    role: "status",
    "data-kp-state": "loading",
    "aria-label": label,
    style: `block-size: ${height}`,
  });

/**
 * An empty state that teaches (FLOWS.md §1.5, DESIGN_LANGUAGE §7): one
 * sentence saying why it is empty and what fills it, plus the action that
 * fills it.
 * @param {{title: string, text: string, action?: Node | null,
 *   art?: Node | null}} spec
 */
export function emptyState(spec) {
  return h(
    "div",
    { class: "kp-empty nx-empty" },
    ...(spec.art ? [spec.art] : []),
    h("p", { class: "kp-empty__title" }, spec.title),
    h("p", { class: "kp-empty__body" }, spec.text),
    ...(spec.action ? [spec.action] : []),
  );
}

/**
 * FNV-1a over a name: the seed of its identity mark.
 * @param {string} s
 */
export function nameHash(s) {
  let x = 2166136261;
  for (const c of s) x = Math.imul(x ^ c.charCodeAt(0), 16777619);
  return x >>> 0;
}

/**
 * A stack's identity mark as data: its hue and the filled cells of a 5×5
 * pattern mirrored around the middle column (graphics.html, approved):
 * deterministic, so the same stack looks the same everywhere.
 * @param {string} name
 * @returns {{hue: number, cells: [number, number][]}}
 */
export function markCells(name) {
  const hsh = nameHash(name);
  /** @type {[number, number][]} */
  const cells = [];
  for (let y = 0; y < 5; y++)
    for (let x = 0; x < 3; x++)
      if ((hsh >>> (y * 3 + x + 4)) & 1) {
        cells.push([x, y]);
        if (x < 2) cells.push([4 - x, y]);
      }
  return { hue: hsh % 360, cells };
}

const SVG = "http://www.w3.org/2000/svg";

/**
 * The per-stack identity mark (DESIGN_LANGUAGE §11): an SVG square in the
 * stack's own hue with its mirrored pattern; decorative, so hidden from a
 * screen reader (the name always stands beside it).
 * @param {string} name
 * @param {number} [size] px
 * @returns {SVGSVGElement}
 */
export function stackMark(name, size = 20) {
  const { hue, cells } = markCells(name);
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", "nx-mark");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("aria-hidden", "true");
  svg.dataset.stack = name;
  svg.style.setProperty("--mark-hue", String(hue));
  svg.style.borderRadius = `${size / 4.4}px`;
  const bg = document.createElementNS(SVG, "rect");
  bg.setAttribute("class", "nx-mark__ground");
  bg.setAttribute("width", "100");
  bg.setAttribute("height", "100");
  svg.append(bg);
  const u = 100 / 7;
  for (const [x, y] of cells) {
    const r = document.createElementNS(SVG, "rect");
    r.setAttribute("class", "nx-mark__cell");
    r.setAttribute("x", String((x + 1) * u));
    r.setAttribute("y", String((y + 1) * u));
    r.setAttribute("width", String(u));
    r.setAttribute("height", String(u));
    svg.append(r);
  }
  return svg;
}

/**
 * A stack's name with its mark in front, for a table cell or a header.
 * @param {string} name
 * @param {{href?: string, size?: number}} [opts]
 */
export function stackName(name, opts = {}) {
  return h(
    "span",
    { class: "nx-stackname" },
    stackMark(name, opts.size ?? 20),
    opts.href ? h("a", { href: opts.href }, name) : h("span", null, name),
  );
}

/**
 * The points of a sparkline in a w×h box.
 * @param {number[]} values
 * @param {number} [w]
 * @param {number} [hgt]
 * @returns {{line: string, area: string}}
 */
export function sparkPath(values, w = 100, hgt = 28) {
  const lo = Math.min(...values);
  const hi = Math.max(...values);
  const span = hi - lo || 1;
  const pts = values.map((v, i) => [
    (i / Math.max(1, values.length - 1)) * w,
    hgt - 2 - ((v - lo) / span) * (hgt - 4),
  ]);
  const line = `M${pts.map((p) => p.map((n) => n.toFixed(1)).join(",")).join("L")}`;
  return { line, area: `${line}L${w},${hgt}L0,${hgt}Z` };
}

/**
 * A 28 px sparkline (DESIGN_LANGUAGE §9.2), drawn in `--chart-1` unless
 * told otherwise; the numbers always stand beside it.
 * @param {number[]} values
 * @param {{colour?: string}} [opts]
 * @returns {SVGSVGElement}
 */
export function sparkline(values, opts = {}) {
  const { line, area } = sparkPath(values);
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", "nx-spark");
  svg.setAttribute("viewBox", "0 0 100 28");
  svg.setAttribute("preserveAspectRatio", "none");
  svg.setAttribute("aria-hidden", "true");
  if (opts.colour) svg.style.color = opts.colour;
  const a = document.createElementNS(SVG, "path");
  a.setAttribute("class", "area");
  a.setAttribute("d", area);
  const l = document.createElementNS(SVG, "path");
  l.setAttribute("class", "line");
  l.setAttribute("d", line);
  svg.append(a, l);
  return svg;
}
