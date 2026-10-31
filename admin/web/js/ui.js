// feat-shell-5 (redesign 3.71.0, DESIGN_LANGUAGE.md §1-§12, Kenny approved
// 2026-10-03): the shared building blocks every redesigned page is made
// of, so the pages read as one product. kp-themes components underneath
// (kp-card, kp-alert, kp-badge, kp-breadcrumb, kp-skeleton, kp-empty,
// kp-button) and only kp-themes tokens in app.css's `nx-` layout classes;
// no second design system.
//
//   pageHeader   title (+ a muted `sub`), one-sentence description,
//                breadcrumbs, live status, the actions with ONE primary and
//                an overflow slot; `meta` adds the chips row under the
//                description with the actions spanning the header's rows,
//                `split` puts the live status beside the title (next.css),
//                `level: "h2"` when another page carries the h1
//   kpiStrip     3-6 KPI tiles: label, value, context line, a sparkline or
//                a meter, each tile a link to its filtered detail or a
//                toggle of a filter on the page
//   attention    one kp-alert per problem, worst first, each with its fix;
//                zero height when all is well
//   toggleGroup  the plain-click toggle group (several on, every one on is
//                All, Esc / All / Show all resets), segmented;
//                segSwitch the one-of switch
//   section      a card: heading + one sentence + tools, optionally
//                collapsible and mounted only when first opened, with a
//                foot line (source left, "read 4 s ago" right)
//   (tables)     every table is a kp datatable (dataTable / dom.js
//                tableBlock); sorting lives in sortstate.js
//   toast        kp-themes' toast in a region inside #page, so Live view
//                can press its Undo
//   rowMenu / moreMenu   a row's menu on a button already in the row and a
//                header's more menu (the `···` one, or grouped under
//                headings like the stack hub's More ▾), both kp-themes'
//                menu button
//   drawer       a side panel, kp-themes' `dialog.kp-drawer`
//   chip / dot / meter / swatch   small marks
//                (kp-badge, kp-health, kp-meter)
//   failBox / emptyState   the error and empty states in the block's own
//                footprint (loading is the component's own state, or stage.js)
//   kbd / keyRow   keyboard: a key, the shortcut line
//   sparkline    a 28 px trend line for a tile or a table row
//   doneMark     the big done tick (the Update flow, the Inbox)
//   art          the small empty and all-clear line drawings of
//                graphics.html, for emptyState's `art`
//
// Every block that draws something clickable takes the Live view control
// it is (a declared id, or a dialog control name) from its caller, so no
// page grows an undeclared control (invariant 39).
//
// Two looks of the same blocks, both approved demos, and app.css draws
// both: the default one is the demos' next.css (Secrets; Backups too, with
// `.bk-page` where its demo measures differently), and `.nx-ops` on a
// page's root is ops-kit.css's (Host and Schedules today) — flush cards
// with a ruled foot, a segmented control, a quieter KPI. A page picks its
// look by its demo; the blocks never ask.
//
// The shared time chart lives in charts.js (kp-themes' own chart.js).

import { agoEl, setAgo } from "./ago.js";
import { h, setLive } from "./dom.js";
import { dialogControl, drivable } from "./drivable.js";
import {
  attachDataTables,
  toast as kpToast,
  setAttention,
  statePlate,
  KPI_TOGGLE_EVENT,
  drawSparkline,
  setMeter,
  MENU_SELECT_EVENT,
  attachMenuButtons,
  closeMenu,
  openMenu,
  setMenu,
  SEGMENTED_EVENT,
  attachSegmented,
  getSegmentedValues,
  itemsOf,
  resetSegmented,
  setSegmentedCounts,
  setSegmentedItems,
  setSegmentedValues,
  valueOf,
  setKeys,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * The Live view control a clickable block is: a declared id (drivable.js),
 * with its row when the control repeats per row.
 * @typedef {{id: string, row?: string}} Drive
 */

/** @param {HTMLElement} e @param {Drive} d */
const drive = (e, d) => drivable(e, d.id, d.row);

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
 * The live freshness of a page or a card: kp-themes' status dot
 * (`.kp-health`) and "updated 4 s ago", ticking (ago.js; the theme draws
 * the stale plate once it is older than three minutes).
 * `set(at, words)`: `words` stand in for the time while something is under
 * way ("reading 3 of 6 stacks…", `aria-busy`); `data-state` is kp-health's:
 * `ok` once read, `down` after `fail`, none before.
 * @param {string} [verb] "updated", "read", "measured"
 * `fail(words)`: the read failed ("not read"), a red dot.
 * @returns {{el: HTMLElement,
 *   set: (at: number | null, words?: string) => void,
 *   fail: (words: string) => void}}
 */
export function liveStatus(verb = "updated") {
  const ago = agoEl(verb, null, { live: true });
  // The words are the live region; kp's ticking line must stay out of one.
  const words = h("span", { class: "nx-live__words", role: "status" });
  const e = h(
    "span",
    { class: "kp-health nx-live" },
    h("span", { class: "kp-health__dot", "aria-hidden": "true" }),
    ago,
    words,
  );
  /** @param {"ok" | "down" | null} state kp-health's own words; none: muted */
  const say = (state) => {
    if (state) e.dataset.state = state;
    else delete e.dataset.state;
  };
  /** @param {number | null} at @param {string} [w] */
  const set = (at, w) => {
    setAgo(ago, at);
    ago.hidden = w != null;
    words.hidden = w == null;
    words.textContent = w ?? "";
    e.toggleAttribute("aria-busy", w != null);
    say(w == null && at != null ? "ok" : null);
  };
  set(null);
  /** @param {string} w */
  const fail = (w) => {
    set(null, w);
    e.removeAttribute("aria-busy");
    say("down");
  };
  return { el: e, set, fail };
}

/**
 * @typedef {{title: string, sub?: string, desc: string, crumbs?: Crumb[],
 *   live?: string | boolean, actions?: Node[], primary?: Node | null,
 *   more?: Node | null, meta?: Node[], level?: "h1" | "h2",
 *   split?: boolean, titleMeta?: Node[]}} HeaderSpec
 *   `titleMeta`: chips right beside the title in its row (the Configure
 *   pages' "working copy bb5dce9" / "read 3 s ago"), the actions still
 *   against the right edge;
 *   `sub`: a muted part of the title ("Host demo");
 *   `split`: the next.css header (Secrets); every header is that shape
 *   since kp-themes 9.2.0's `.kp-page-header`;
 *   `level`: the title's heading level, `h1` by default; `h2` when the
 *   page is embedded under another page's title (Schedules as Activity's
 *   "Planned" view);
 *   `live`: show a live status (true, or the verb: "updated", "read");
 *   `actions`: at most two secondary actions;
 *   `primary`: the ONE primary action, after them;
 *   `more`: the overflow menu (`moreMenu`), last;
 *   `meta`: chips and facts in a row under the description (the live
 *   status joins them).
 */

let countIds = 0;
/**
 * redesign-final (exact counts, Kenny's rule): link a count to the rows it
 * describes, declaratively, so a whole-screen check can hold the number to
 * the rows (`data-count-of` names the list, `data-count-rows` the rows
 * inside it). The count's first number is the rows drawn: "N" or "N of M".
 * @param {HTMLElement} count
 * @param {HTMLElement} list
 * @param {string} [rows] the rows' selector inside `list`
 */
export function countOf(count, list, rows = ":scope > *") {
  if (!list.id) list.id = `nx-counted-${++countIds}`;
  count.dataset.countOf = list.id;
  count.dataset.countRows = rows;
}

/**
 * The page header (DESIGN_LANGUAGE §1.1) on kp-themes 9.2.0's
 * `.kp-page-header`: breadcrumbs, then the title with its live status and
 * the page's one-sentence description right under it (and the meta row) on
 * the left, the actions against the right edge — secondaries, the ONE
 * primary, the more menu last; on a narrow header it reads top to bottom,
 * the primary first at the full width. Markup the whole-screen invariants
 * read: `.kp-page-header__line` holding the `h1`, the description its next sibling
 * `p` (rows 32, 36, 40, 41), the actions `.nx-head-actions`.
 * @param {HeaderSpec} spec `split` is the same header since kp-themes 9.2.0
 * @returns {{el: HTMLElement, title: HTMLHeadingElement,
 *   desc: HTMLParagraphElement, actions: HTMLElement,
 *   meta: HTMLElement | null,
 *   live: ReturnType<typeof liveStatus> | null}}
 */
export function pageHeader(spec) {
  const title = h(
    spec.level ?? "h1",
    { class: "kp-page-header__title" },
    spec.title,
    ...(spec.sub ? [h("span", { class: "nx-head-sub" }, ` ${spec.sub}`)] : []),
  );
  const live =
    spec.live === undefined || spec.live === false
      ? null
      : liveStatus(typeof spec.live === "string" ? spec.live : "updated");
  const actions = h(
    "div",
    { class: "kp-page-header__actions nx-head-actions" },
    ...(spec.actions ?? []),
    ...(spec.primary ? [spec.primary] : []),
    ...(spec.more ? [spec.more] : []),
  );
  const desc = h(
    "p",
    { class: "kp-page-header__description section-head__desc nx-head-desc" },
    spec.desc,
  );
  // The trail goes BEFORE `.kp-page-header__inner` (kp-themes
  // `.kp-page-header__back`), so it is not the left cell's first baseline.
  const crumbs = spec.crumbs?.length ? [breadcrumbs(spec.crumbs)] : [];
  for (const c of crumbs) c.classList.add("kp-page-header__back");
  // titleMeta: chips right beside the title in its row (the Configure
  // pages' "working copy bb5dce9"); meta: a row under the description.
  const titleMeta = spec.titleMeta
    ? h("div", { class: "nx-head-titlemeta" }, ...spec.titleMeta)
    : null;
  const meta = spec.meta
    ? h("div", { class: "nx-head-meta" }, ...spec.meta)
    : null;
  // redesign-final X1: the page's freshness ("updated 4 s ago") has one
  // slot on every page, as the demos draw it: right after the title.
  const row = h(
    "div",
    { class: "kp-page-header__heading" },
    h(
      "div",
      { class: "kp-page-header__line" },
      title,
      live?.el ?? null,
      titleMeta,
    ),
  );
  const e = h(
    "header",
    { class: `kp-page-header nx-head${meta ? " nx-head--meta" : ""}` },
    ...crumbs,
    h(
      "div",
      { class: "kp-page-header__inner" },
      h("div", { class: "nx-head-lead" }, row, desc, meta),
      actions,
    ),
  );
  return { el: e, title, desc, actions, meta: meta ?? titleMeta, live };
}

/**
 * @typedef {{pct: number | null, mark?: number | null, markTitle?: string,
 *   tone?: "" | "warn" | "bad" | null, label?: string}} Meter
 *   a bar `pct` percent full (kp-themes' `.kp-meter`: a share past 100 fills
 *   the bar and says so), with a tick at `mark` percent (what is promised,
 *   a limit; `markTitle` names it for a screen reader), in the warn / bad
 *   colour when `tone` says so; `label`: what the share is ("full")
 */

/**
 * kp-themes' word for a dashboard tone, on a tile, a meter or a badge.
 * @type {Record<string, "warning" | "destructive">}
 */
const KP_TONE = { warn: "warning", bad: "destructive" };

/**
 * Write a dashboard `Meter` into a kp-themes meter element (`setMeter`:
 * the share, the mark, the tone and the ARIA in one call).
 * @param {HTMLElement} el
 * @param {Meter | null | undefined} m
 */
function paintMeter(el, m) {
  setMeter(el, {
    value: m?.pct == null ? null : m.pct / 100,
    mark: m?.mark == null ? null : m.mark / 100,
    tone: m?.tone ? (KP_TONE[m.tone] ?? null) : null,
    ...(m?.label ? { label: m.label } : {}),
    ...(m?.markTitle ? { markLabel: m.markTitle } : {}),
  });
}

/**
 * @typedef {{key?: string, label: string, value?: string, unit?: string,
 *   ctx?: string, ctxTone?: "ok" | "warn" | "bad" | "" | null,
 *   ctxParts?: {text: string, tone?: "ok" | "warn" | "bad" | ""}[] | null,
 *   colour?: string, target?: string, ctxMeter?: Meter | null,
 *   tone?: "" | "warn" | "bad" | null, href?: string,
 *   spark?: number[], meter?: Meter | null, title?: string,
 *   toggle?: {pressed: boolean, onToggle: () => void, drive?: Drive}}} Kpi
 *   `spark` or `meter` fill the tile's fourth row (a tile given neither
 *   has three rows). `href`: the tile links to its filtered detail;
 *   `toggle`: the tile is a button turning a filter on this page on or
 *   off (`aria-pressed`; kp-themes' `attachKpiToggles` flips it and fires
 *   `kp-kpi-toggle`), the Live view control `drive` (required when the
 *   tile is made; a later `set` may leave it out). `ctxTone` puts a
 *   status dot before the context line; `ctxParts` draws the context as
 *   pieces, each with its own dot ("7 enforced" green, "2 open" amber).
 *   `colour`: the sparkline's colour. `target`: the id of the card on this
 *   page the tile sums up; the tile is a same-site link to it and scrolls
 *   there (Metrics, the Map). `ctxMeter`: a small used-of-limit bar at
 *   the start of the context line (the Stacks demo's tiles). A tile made without a spark or meter grows
 *   its fourth row when a later `set` brings one.
 */

/**
 * A pulsing placeholder of a given width (DESIGN_LANGUAGE §7), inline.
 * @param {string} [width]
 */
export function skeleton(width = "70%") {
  return h("span", {
    class: "kp-skeleton nx-sk",
    style: `inline-size: ${width}`,
    "aria-hidden": "true",
  });
}

/**
 * One KPI tile (DESIGN_LANGUAGE §1.2) as kp-themes draws a key figure
 * (`.kp-kpi`): label, value (tabular), a context line, an optional
 * sparkline or meter; a tile with an `href` is a link to its filtered
 * detail, one with a `toggle` a button. `set` repaints it in place, so the
 * strip never reflows (rule 6); `set({loading: true})` shows skeletons in
 * the value and the context.
 * @param {Kpi} k
 * @returns {{el: HTMLElement,
 *   set: (k: Partial<Kpi> & {loading?: boolean}) => void}}
 */
export function kpi(k) {
  const label = h("span", { class: "kp-kpi__label" });
  const hint = h(
    "span",
    { class: "kp-kpi__hint", "aria-hidden": "true" },
    h("span", { "data-kp-when": "off" }, "filter"),
    h("span", { "data-kp-when": "on" }, "filtering ×"),
  );
  const value = h("span", { class: "kp-kpi__value" });
  const num = h("span");
  const unitEl = h("small");
  const ctx = h("span", { class: "kp-kpi__trend" });
  /** @type {HTMLElement | null} the context line's used-of-limit bar */
  let ctxBarEl = null;
  /** @type {string | null} what the context line's words say, to skip an equal repaint */
  let ctxWords = null;
  /** The fourth row: a kp meter or a kp sparkline (an svg). @type {HTMLElement | SVGSVGElement | null} */
  let foot = k.meter ? tileMeter() : k.spark ? tileSpark() : null;
  // A same-site path (invariant 35), the card's id as its hash.
  const href = k.target
    ? `${location.pathname}${location.search}#${k.target}`
    : k.href;
  /** @type {HTMLElement} */
  const e = k.toggle
    ? h("button", { type: "button", class: "kp-kpi kp-kpi--toggle" })
    : h(href ? "a" : "div", {
        class: "kp-kpi",
        ...(href ? { href } : {}),
      });
  e.append(label, ...(k.toggle ? [hint] : []), value, ctx);
  if (foot) e.append(foot);
  /** @type {Kpi} */
  let cur = { ...k };
  if (k.target)
    e.addEventListener("click", (ev) => {
      if (!cur.target) return;
      // The page's own scroll, never a navigation (main.js routes links).
      ev.preventDefault();
      document
        .getElementById(cur.target)
        ?.scrollIntoView({ block: "start", behavior: "smooth" });
    });
  if (k.toggle) {
    if (!k.toggle.drive)
      throw new Error(`the KPI toggle ${k.label} needs its Live view control`);
    drive(e, k.toggle.drive);
    // kp's attachKpiToggles flips `aria-pressed` and says so; the page keeps
    // the filter, and `set` writes its state back.
    e.addEventListener(KPI_TOGGLE_EVENT, () => cur.toggle?.onToggle());
  }
  /** @returns {HTMLElement} */
  function tileMeter() {
    return h("span", { class: "kp-kpi__meter" });
  }
  /** @returns {SVGSVGElement} */
  function tileSpark() {
    const svg = /** @type {SVGSVGElement} */ (
      document.createElementNS(SVG, "svg")
    );
    svg.setAttribute("class", "kp-kpi__spark");
    svg.setAttribute("aria-hidden", "true");
    return svg;
  }
  /**
   * The fourth row holds a meter or a sparkline. Once a tile has the row it
   * keeps it (a tile never shrinks); a meter that went away leaves a
   * sparkline's row, a spark that arrives late makes the row.
   */
  const paintFoot = () => {
    const spark = cur.spark && cur.spark.length > 1 ? cur.spark : null;
    /** @param {HTMLElement | SVGSVGElement} next */
    const put = (next) => {
      if (foot) foot.replaceWith(next);
      else e.append(next);
      foot = next;
    };
    if (cur.meter) {
      if (!foot || isSvg(foot)) put(tileMeter());
      paintMeter(/** @type {HTMLElement} */ (foot), cur.meter);
    } else if (spark || foot) {
      if (!foot || !isSvg(foot)) put(tileSpark());
      const svg = /** @type {SVGSVGElement} */ (foot);
      svg.setAttribute("data-kp-spark", (spark ?? []).join(" "));
      if (cur.colour) svg.style.color = cur.colour;
      else svg.style.removeProperty("color");
      drawSparkline(svg, spark ?? []);
    }
  };
  let loading = false;
  const set = (/** @type {Partial<Kpi> & {loading?: boolean}} */ next) => {
    const { loading: l, ...rest } = next;
    // A value arriving ends the skeleton unless the caller says otherwise.
    if (l != null) loading = l;
    else if ("value" in rest) loading = false;
    cur = { ...cur, ...rest };
    label.textContent = cur.label;
    if (loading) e.dataset.loading = "";
    else delete e.dataset.loading;
    if (loading) value.replaceChildren(skeleton("3ch"));
    else {
      // A figure that changed plays the theme's update (kp-themes'
      // update()); the first fill after the skeleton is written plainly.
      const text = cur.value ?? "—";
      if (num.parentNode === value && num.textContent !== text)
        setLive(num, text);
      else num.textContent = text;
      // refresh-no-replay-1: the figure keeps its element; only the unit
      // beside it comes and goes.
      if (num.parentNode !== value) value.replaceChildren(num);
      if (cur.unit) {
        if (!unitEl.parentNode) value.append(unitEl);
        if (unitEl.textContent !== cur.unit) unitEl.textContent = cur.unit;
      } else unitEl.remove();
    }
    // The context line: a used-of-limit bar, then words. A refresh that
    // brings other numbers repaints the bar and the words where they stand;
    // the bar is a new element only when the line gains one (the theme wipes
    // a new meter in, so a redrawn one would wipe in again every minute).
    const wantBar = !loading && cur.ctxMeter ? cur.ctxMeter : null;
    if (wantBar) {
      if (!ctxBarEl) {
        ctxBarEl = ctxBar(wantBar, cur.label);
        ctx.prepend(ctxBarEl);
      } else {
        paintMeter(ctxBarEl, wantBar);
        ctxBarEl.setAttribute("aria-label", cur.label);
      }
    } else if (ctxBarEl) {
      ctxBarEl.remove();
      ctxBarEl = null;
    }
    const words = JSON.stringify([loading, cur.ctx, cur.ctxTone, cur.ctxParts]);
    if (words !== ctxWords) {
      ctxWords = words;
      for (const n of [...ctx.childNodes]) if (n !== ctxBarEl) n.remove();
      ctx.append(
        ...(loading
          ? [skeleton("80%")]
          : cur.ctxParts?.length
            ? cur.ctxParts.flatMap((p, i) => [
                ...(i ? [" · "] : []),
                p.tone ? dot(p.tone, p.text) : p.text,
              ])
            : [
                cur.ctxTone ? dot(cur.ctxTone, cur.ctx ?? "") : (cur.ctx ?? ""),
              ]),
      );
    }
    // kp-themes' tone: a coloured edge, the figure on its status plate.
    const tone = cur.tone ? KP_TONE[cur.tone] : undefined;
    if (tone) e.dataset.kpTone = tone;
    else delete e.dataset.kpTone;
    e.dataset.key = cur.key ?? cur.label;
    if (cur.title) e.title = cur.title;
    if (cur.toggle) e.setAttribute("aria-pressed", String(cur.toggle.pressed));
    paintFoot();
    if (cur.href && e instanceof HTMLAnchorElement) e.href = cur.href;
  };
  set({});
  return { el: e, set };
}

/**
 * A tile's inline meter (`.kp-meter--inline`), read out as its percentage.
 * @param {Meter} m @param {string} label
 */
function ctxBar(m, label) {
  const e = meter(m, "kp-meter kp-meter--inline").el;
  e.setAttribute("aria-label", label);
  return e;
}

/**
 * A row of 3 to 6 KPI tiles (`.kp-kpis`, every tile as tall as the
 * tallest); kp-themes' `attachKpiStrips` takes it to the column count that
 * fits its own width. While `loading` each tile is its own skeleton in the
 * final geometry.
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean, label?: string, drive?: {id: string}}} [opts]
 *   `drive`: the Live view control every tile is, on its own row (the
 *   tile's key)
 * @returns {{el: HTMLElement, tiles: Map<string, ReturnType<typeof kpi>>}}
 */
export function kpiStrip(tiles, opts = {}) {
  /** @type {Map<string, ReturnType<typeof kpi>>} */
  const map = new Map();
  const e = h("div", {
    class: "kp-kpis",
    "data-kp-kpis-columns": "all 3 2 1",
    role: "group",
    "aria-label": opts.label ?? "Key figures",
  });
  for (const t of tiles) {
    const k = kpi(t);
    if (opts.drive) drive(k.el, { id: opts.drive.id, row: t.key ?? t.label });
    if (opts.loading) k.set({ loading: true });
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
 * The attention band (DESIGN_LANGUAGE §1.3): kp-themes' `.kp-attention`,
 * one alert per problem, worst first, each with its own fix; no room at
 * all, never "all fine" filler, when nothing needs a person. `set` goes by
 * each item's key (kp's `setAttention`): an unchanged problem stays the
 * same element, so it is announced once and a focused fix keeps its
 * focus; a new one arrives and a solved one leaves, the theme's way.
 * @param {Attention[]} [items]
 * @returns {{el: HTMLElement, set: (items: Attention[]) => void}}
 */
export function attentionBand(items = []) {
  const el = h("div", {
    class: "kp-attention nx-attention",
    role: "region",
    "aria-label": "Needs a look",
  });
  /** @type {Record<Attention["tone"], "critical" | "warning" | "info">} */
  const severity = { bad: "critical", warn: "warning", info: "info" };
  const set = (/** @type {Attention[]} */ list) =>
    setAttention(
      el,
      list.map((a) => ({
        key: a.key ?? a.title,
        severity: severity[a.tone],
        title: a.title,
        ...(a.text ? { text: a.text } : {}),
        action: a.action ?? null,
      })),
    );
  set(items);
  return { el, set };
}

/**
 * @typedef {{title?: string, desc?: string, label?: string, id?: string,
 *   tools?: Node[], badge?: Node | null, foot?: import("./dom.js").Child[],
 *   body?: import("./dom.js").Child, collapsible?: boolean, open?: boolean,
 *   mount?: (body: HTMLElement) => (() => void) | void,
 *   level?: "h2" | "h3", tag?: "section" | "aside" | "nav", cls?: string,
 *   plain?: boolean}}
 *   SectionSpec
 *   `title` + `desc`: the head (rule 8); a card without a title (a hero)
 *   names itself with `label` instead. `badge`: a count beside the title.
 *   `foot`: the foot line, each item a span spread from left to right
 *   (the source, then "read 4 s ago"); without it the returned `foot` is
 *   hidden, for a caller that fills it later. `body`: children to start
 *   with; `mount`: fills the body the first time the card is open.
 *   `plain`: a flat card without kp-card's themed frame (no corner cut or
 *   glow in the dark themes), for a page whose demo draws plain cards.
 */

/**
 * A section of a page as a kp card (DESIGN_LANGUAGE §5): a header row with
 * its title and one sentence (rule 8) on the left and its tools on the
 * right, then the body, then an optional foot line. `collapsible` makes
 * it a `<details>` whose summary is the whole head, tools included
 * (rarely used or destructive things go last, folded); `mount` fills the
 * body, the first time it is open — so a folded section never pays for
 * its read — and its cleanup runs with `stop()`.
 * @param {SectionSpec} spec
 * @returns {{el: HTMLElement, head: HTMLElement, body: HTMLElement,
 *   foot: HTMLElement, title: HTMLElement | null,
 *   desc: HTMLElement | null, tools: HTMLElement | null,
 *   setFoot: (parts: import("./dom.js").Child[]) => void,
 *   stop: () => void, open: () => void}}
 *   `title`, `desc`, `tools`: the head's parts, for a card whose heading
 *   follows what it shows (a stack's name)
 */
export function section(spec) {
  const level = spec.level ?? "h2";
  const titleEl = spec.title
    ? h(level, spec.id ? { id: `${spec.id}-h` } : null, spec.title)
    : null;
  const descEl = titleEl
    ? h("p", { class: "section-head__desc" }, spec.desc ?? "")
    : null;
  const toolsEl =
    titleEl && (spec.tools?.length || spec.collapsible)
      ? h(
          "div",
          { class: "nx-card__tools" },
          ...(spec.tools ?? []),
          ...(spec.collapsible
            ? [h("span", { class: "nx-chev", "aria-hidden": "true" }, "›")]
            : []),
        )
      : null;
  const head = titleEl
    ? h(
        "div",
        { class: "nx-card__head" },
        titleEl,
        spec.badge ?? null,
        descEl,
        toolsEl,
      )
    : h("span", { class: "nx-card__nohead" });
  const body = h("div", { class: "nx-card__body" }, spec.body ?? null);
  const foot = h(
    "div",
    { class: "nx-card__foot" },
    (spec.foot ?? []).map((x) => h("span", null, x)),
  );
  foot.hidden = !spec.foot?.length;
  /** @type {(() => void) | null} */
  let cleanup = null;
  let mounted = false;
  const fill = () => {
    if (mounted || !spec.mount) return;
    mounted = true;
    cleanup = spec.mount(body) ?? null;
  };
  /** @type {Record<string, string>} */
  const named = titleEl?.id
    ? { "aria-labelledby": titleEl.id }
    : spec.title || spec.label
      ? { "aria-label": spec.title || spec.label || "" }
      : {};
  const cls = (/** @type {string} */ c) =>
    `${spec.plain ? "nx-card nx-card--plain" : "kp-card nx-card"}${c}${spec.cls ? ` ${spec.cls}` : ""}`;
  /** @type {HTMLElement} */
  let e;
  if (spec.collapsible) {
    const d = /** @type {HTMLDetailsElement} */ (
      h(
        "details",
        {
          // kp's bare accordion item (port spec J3): the fold glides
          // open and shut the theme's way, without the accordion's chrome.
          class: cls(
            " nx-card--fold kp-accordion__item kp-accordion__item--bare",
          ),
          ...(spec.id ? { id: spec.id } : {}),
          ...named,
          ...(spec.open ? { open: "" } : {}),
        },
        h("summary", null, head),
        body,
        foot,
      )
    );
    d.addEventListener("toggle", () => {
      if (d.open) fill();
    });
    if (spec.open) fill();
    e = d;
  } else {
    e = h(
      spec.tag ?? "section",
      { class: cls(""), ...(spec.id ? { id: spec.id } : {}), ...named },
      head,
      body,
      foot,
    );
    fill();
  }
  return {
    el: e,
    head,
    body,
    foot,
    title: titleEl,
    desc: descEl,
    tools: toolsEl,
    // The foot line drawn later (the source left, a link or "read 4 s
    // ago" right), when it is known only after the section is made.
    setFoot: (parts) => {
      foot.replaceChildren(...parts.map((x) => h("span", null, x)));
      foot.hidden = !parts.length;
    },
    stop: () => cleanup?.(),
    open: () => {
      if (e instanceof HTMLDetailsElement) e.open = true;
      fill();
    },
  };
}

/**
 * An empty state that teaches (FLOWS.md §1.5, DESIGN_LANGUAGE §7): one
 * sentence saying why it is empty and what fills it, plus the action that
 * fills it.
 * @param {{title: string, text: string, action?: Node | null,
 *   art?: Node | null}} spec
 */
export function emptyState(spec) {
  // The package's own empty plate (centred, the title at heading size); its
  // `nx-empty` class stays so a page can find the one it drew.
  const plate = statePlate(document, {
    kind: "empty",
    title: spec.title,
    text: spec.text,
    ...(spec.art ? { art: spec.art } : {}),
  });
  plate.classList.add("nx-empty");
  if (spec.action) plate.append(spec.action);
  return plate;
}

const SVG = "http://www.w3.org/2000/svg";

/** @param {Element} e */
const isSvg = (e) => e.namespaceURI === SVG;

/**
 * A 28 px sparkline (DESIGN_LANGUAGE §9.2), drawn in `--chart-1` unless
 * told otherwise; the numbers always stand beside it.
 * @param {number[]} values
 * @param {{colour?: string}} [opts]
 * @returns {SVGSVGElement}
 */
export function sparkline(values, opts = {}) {
  // kp-themes' drawSparkline (js/kpi.js), the line its KPI tiles and time
  // charts draw; `data-kp-spark` holds the numbers, so update() can move
  // its last point the theme's way.
  const svg = /** @type {SVGSVGElement} */ (
    document.createElementNS(SVG, "svg")
  );
  svg.setAttribute("class", "nx-spark");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("data-kp-spark", values.join(" "));
  if (opts.colour) svg.style.color = opts.colour;
  drawSparkline(svg, values, { parts: "nx-spark" });
  return svg;
}

/**
 * A plain button. `cls` adds kp modifiers ("kp-button--primary", …).
 * @param {string} label
 * @param {{cls?: string, title?: string, onClick?: (e: MouseEvent) => void,
 *   attrs?: Record<string, string>}} [o]
 */
export function button(label, o = {}) {
  const b = h(
    "button",
    {
      type: "button",
      class: `kp-button ${o.cls ?? ""}`.trim(),
      ...(o.title ? { title: o.title } : {}),
      ...(o.attrs ?? {}),
    },
    label,
  );
  if (o.onClick) b.addEventListener("click", o.onClick);
  return b;
}

/**
 * One value of a plain-click toggle group turned on or off: several may be
 * on, and every value on is the same as none (back to All).
 * @param {Set<string>} on
 * @param {string} value
 * @param {number} total how many values the group has
 * @returns {Set<string>}
 */
export function toggleValue(on, value, total) {
  const next = new Set(on);
  if (next.has(value)) next.delete(value);
  else next.add(value);
  if (next.size >= total) next.clear();
  return next;
}

/**
 * @typedef {{value: string, label: string, count?: number | null,
 *   hint?: string}} Chip
 */

/**
 * @typedef {{label: string, chips: Chip[], onChange: (on: Set<string>) => void,
 *   drive: {id: string}, selected?: Iterable<string>,
 *   all?: {label: string, hint?: string} | null}} ToggleSpec
 *   `drive`: the Live view control every button is, on its own value's
 *   row ("all" for All) — required, a toggle nobody can press from Live
 *   view is a defect (invariant 39). `all`: an "All" button first, pressed
 *   while nothing is on.
 */

/**
 * The plain-click toggle group (DESIGN_LANGUAGE §10, §12; Kenny
 * 2026-10-03: "of meerdere kunnen aanklikken"): each click turns one value
 * on or off, several may be on, no modifier keys; none on means everything
 * is shown, and turning the last one on is the same as none — the group
 * goes back to All (one rule for every look). `reset()` — All, Show all,
 * Esc — turns them all off; counts sit beside each value, exact.
 * @param {ToggleSpec} spec
 * @returns {{el: HTMLElement, selected: () => Set<string>,
 *   set: (values: Iterable<string>) => void, reset: () => void,
 *   counts: (n: Record<string, number | null | undefined>) => void,
 *   setChips: (chips: Chip[]) => void}}
 */
export function toggleGroup(spec) {
  if (!spec.drive?.id)
    throw new Error(
      `the toggle group ${spec.label} needs its Live view control`,
    );
  const driveId = spec.drive.id;
  /** @type {Chip[]} */
  let chips = spec.chips;
  /** @type {Record<string, number | null | undefined>} */
  let n = Object.fromEntries(chips.map((c) => [c.value, c.count]));
  const group = segmentedGroup(spec.label, "multi", "kp-segmented");
  /** The items as kp draws them: All first, each with its hint and count. */
  const items = () => [
    ...(spec.all
      ? [
          {
            value: "all",
            label: spec.all.label,
            hint: spec.all.hint ?? "Show every row",
            count: n.all ?? null,
            all: true,
          },
        ]
      : []),
    ...chips.map((c) => ({
      value: c.value,
      label: c.label,
      hint: c.hint ?? `Show or hide ${c.label}; each click turns it on or off`,
      count: n[c.value] ?? null,
    })),
  ];
  /** @param {Iterable<string>} values */
  const keep = (values) => {
    setSegmentedItems(group, items());
    for (const b of itemsOf(group)) drive(b, { id: driveId, row: valueOf(b) });
    setSegmentedValues(
      group,
      [...values].filter((v) => chips.some((c) => c.value === v)),
    );
  };
  keep(spec.selected ?? []);
  group.addEventListener(SEGMENTED_EVENT, () =>
    spec.onChange(new Set(getSegmentedValues(group))),
  );
  return {
    el: group,
    selected: () => new Set(getSegmentedValues(group)),
    set: (values) => setSegmentedValues(group, values),
    reset: () => resetSegmented(group),
    counts: (next) => {
      n = { ...n, ...next };
      setSegmentedCounts(group, next);
    },
    setChips: (next) => {
      chips = next;
      for (const c of chips) if (c.count !== undefined) n[c.value] = c.count;
      keep(getSegmentedValues(group));
    },
  };
}

/**
 * A one-of segmented switch ("Changed" / "All"). Each button is the Live
 * view control `drive.id` on its value's row, or — for a page that marks
 * its own — `mark(button, value)` declares it; one of the two is required.
 * `set(v)` moves the switch without calling `onChange` (the address moved
 * it); `value()` reads it.
 * @param {{label: string, items: {value: string, label: string,
 *   hint?: string}[], value: string, onChange: (v: string) => void,
 *   drive?: {id: string},
 *   mark?: (button: HTMLElement, value: string) => void}} spec
 * @returns {{el: HTMLElement, set: (v: string) => void,
 *   value: () => string,
 *   counts: (n: Record<string, number>) => void}}
 */
export function segSwitch(spec) {
  if (!spec.drive && !spec.mark)
    throw new Error(`the switch ${spec.label} needs its Live view control`);
  const group = segmentedGroup(spec.label, "single", "kp-segmented");
  setSegmentedItems(
    group,
    spec.items.map((it) => ({
      value: it.value,
      label: it.label,
      hint: it.hint,
      count: null,
    })),
  );
  for (const b of itemsOf(group)) {
    if (spec.drive) drive(b, { id: spec.drive.id, row: valueOf(b) });
    else spec.mark?.(b, valueOf(b));
  }
  setSegmentedValues(group, [spec.value]);
  group.addEventListener(SEGMENTED_EVENT, (ev) => {
    const v = /** @type {CustomEvent} */ (ev).detail?.value;
    if (typeof v === "string") spec.onChange(v);
  });
  return {
    el: group,
    set: (v) => setSegmentedValues(group, [v]),
    value: () => getSegmentedValues(group)[0] ?? spec.value,
    counts: (n) =>
      setSegmentedCounts(
        group,
        Object.fromEntries(spec.items.map((it) => [it.value, n[it.value]])),
      ),
  };
}

/**
 * An empty kp-themes segmented group, wired to its own clicks and keys.
 * Whoever draws it fills it with `setSegmentedItems`.
 * @param {string} label @param {"single" | "multi"} mode @param {string} cls
 * @returns {HTMLElement}
 */
function segmentedGroup(label, mode, cls) {
  const group = h("div", {
    class: cls,
    "data-kp-segmented": mode,
    "aria-label": label,
  });
  attachSegmented(group);
  return group;
}

/**
 * Which earlier toasts a new one replaces: by default every plain one but
 * never one still offering its action (an Undo stays reachable); with
 * "all", every one.
 * @param {{action: boolean}[]} shown
 * @param {"plain" | "all"} replace
 * @returns {number[]} their indexes
 */
export const toastsToDrop = (shown, replace) =>
  shown.flatMap((t, i) => (replace === "all" || !t.action ? [i] : []));

/** How long a toast stays by default, in milliseconds. */
export const TOAST_MS = 6000;
/** fix-ux-1: a toast offering an action (Open, Undo) stays at least this
 * long, and while the pointer or focus is on it. */
export const ACTION_TOAST_MS = 15000;

/**
 * How long a toast stays before it leaves by itself.
 * @param {boolean} hasAction
 * @param {number | undefined} ms what the caller asked for
 */
export const toastMs = (hasAction, ms) =>
  hasAction ? Math.max(ms ?? 0, ACTION_TOAST_MS) : (ms ?? TOAST_MS);

/**
 * The page's toast region: kp-themes' `.kp-toasts` live region, kept
 * inside `#page` (created there on first use) so Live view, which finds a
 * page's controls only in `#page` and open dialogs, can press an Undo.
 * @param {HTMLElement | null} [host] a page's own root, else `#page`
 * @returns {HTMLElement}
 */
export function toastRegion(host) {
  const page =
    (host?.isConnected ? host : null) ??
    document.getElementById("page") ??
    document.body;
  let region = /** @type {HTMLElement | null} */ (
    page.querySelector(":scope > .kp-toasts")
  );
  if (!region) {
    region = h("div", {
      class: "kp-toasts nx-toasts",
      role: "status",
      "aria-live": "polite",
      // An Undo toast arrives by its own CSS entrance and never
      // by the page's box motion. The region sits inside #page, which kp's
      // size motion watches; it took each toast added after the first
      // render for "news" and played its arrival as its leave turned
      // around, with the length of the toast's own 6 s Undo clock: the
      // toast stood folded to its 2 px border for 5.4 s and its Undo could
      // not be pressed. "none" leaves the toast to its entrance.
      "data-kp-arrive": "none",
    });
    page.append(region);
  }
  return region;
}

/**
 * A short message at the foot of the screen (DESIGN_LANGUAGE §8): kp-themes'
 * own `toast()` in the page's region (`toastRegion`), gone after `ms`; with
 * an `action`, its button (Undo) runs it and closes the toast, and is the
 * declared Live view control `action.drive`. Returns a function that
 * closes it now.
 * @param {string} text
 * @param {{action?: {label: string, run: () => void, drive: Drive},
 *   ms?: number, host?: HTMLElement | null,
 *   replace?: "plain" | "all"}} [opts]
 * @returns {() => void}
 */
export function toast(text, opts = {}) {
  const region = toastRegion(opts.host);
  const shown = /** @type {(HTMLElement & {dismiss?: () => void})[]} */ ([
    ...region.children,
  ]);
  for (const i of toastsToDrop(
    shown.map((t) => ({ action: t.dataset.action != null })),
    opts.replace ?? "plain",
  ))
    (shown[i].dismiss ?? (() => shown[i].remove()))();
  const a = opts.action;
  // An Undo, or a toast whose caller fixed `ms`, is bound to a window that
  // really ends (a delete commits): it keeps its time. Any other action
  // (Open, Answer in the Inbox) is held until used or closed (fix-ux-1).
  const held = a != null && opts.ms === undefined && a.label !== "Undo";
  /** @type {(HTMLElement & {dismiss: () => void}) | null} */
  let t = null;
  // kp 11.1.1: a toast stops its clock while the pointer or focus is on it.
  // A held action (Open, Answer) keeps 15 s and has kp's close button; an
  // Undo is bound to a window that really ends, so its clock never pauses.
  t = kpToast(text, {
    region,
    ms: toastMs(held, opts.ms),
    pause: !a || held,
    close: held,
    className: "kp-toast nx-toast",
    ...(a
      ? {
          action: {
            label: a.label,
            onClick: () => {
              t?.dismiss();
              a.run();
            },
          },
        }
      : {}),
  });
  if (a) {
    t.dataset.action = "";
    const b = t.querySelector("button");
    if (b) {
      b.classList.add("nx-toast__act");
      drive(b, a.drive);
    }
    const x = t.querySelector(".kp-toast__close");
    if (x instanceof HTMLElement) dialogControl(x, "toast-close");
  }
  const made = t;
  return () => made.dismiss();
}

/**
 * A row's menu (Edit…, Run now, Delete) on the button already in the row,
 * as kp-themes' menu button (js/menu-button.js): the button is wrapped once
 * in a `.kp-menu-button` with its rich menu beside it, and kp does the rest
 * — it opens on the first entry, closes on a second click, Escape (the
 * focus back on the button), a click outside or a pick, with the theme's
 * own open and close; ↑ ↓ Home End and a first letter move; the menu stays
 * whole on the screen and tells the theme where the button is
 * (`--kp-menu-anchor-x`, `--kp-menu-count`). The page stays usable, and
 * Live view presses an entry by its `name` (drivable.js) while the menu is
 * open. The first call opens the menu; later calls only refill it, as the
 * button's own click is kp's from then on.
 * @param {{anchor: HTMLElement, label: string, title: string,
 *   items: {name: string, label: string, hint: string, run: () => void,
 *   danger?: boolean}[], width?: number}} spec
 * @returns {{el: HTMLElement, close: () => void}}
 */
export function rowMenu(spec) {
  const anchor = spec.anchor;
  const had = anchor.parentElement;
  const made = !had?.hasAttribute("data-kp-row-menu");
  /** @type {HTMLElement} */
  let wrap;
  if (made) {
    const menu = h("div", {
      class: "kp-menu kp-menu--rich",
      role: "menu",
    });
    wrap = h("span", {
      class: "kp-menu-button nx-rowmenu",
      "data-kp-menu-button": "",
      "data-kp-row-menu": "",
    });
    const focused = document.activeElement === anchor;
    // kp's arrival reads `data-kp-arrive` on the element that receives the
    // new child: the wrapper is made around a button already on screen, and
    // its arrival would squeeze it to nothing while the menu, placed at
    // once, measured it (the menu then hung past the button).
    had?.setAttribute("data-kp-arrive", "none");
    anchor.replaceWith(wrap);
    wrap.append(anchor, menu);
    if (focused) anchor.focus({ preventScroll: true });
    wrap.addEventListener(MENU_SELECT_EVENT, (e) => {
      const value = /** @type {CustomEvent<{value: string}>} */ (e).detail
        .value;
      rowItems.get(wrap)?.get(value)?.run();
    });
    attachMenuButtons(wrap.parentElement ?? wrap, {
      decorate: (part, info) => {
        // The entry is the Live view control named for it (dialogControl).
        if (info.kind === "menu-item" && info.value)
          dialogControl(part, info.value);
      },
    });
  } else wrap = /** @type {HTMLElement} */ (had);
  wrap.querySelector("[role=menu]")?.setAttribute("aria-label", spec.label);
  rowItems.set(wrap, new Map(spec.items.map((it) => [it.name, it])));
  setMenu(wrap, [
    {
      group: "",
      items: spec.items.map((it) => ({
        label: it.label,
        hint: it.hint,
        value: it.name,
        ...(it.danger ? { danger: true } : {}),
      })),
    },
  ]);
  if (made) openMenu(wrap);
  return { el: wrap, close: () => closeMenu(wrap) };
}

/** The entries a row menu's wrapper runs, by their name. @type {WeakMap<HTMLElement, Map<string, {run: () => void}>>} */
const rowItems = new WeakMap();

/**
 * One entry of a more menu: a link (`href`) or a button (`onClick`), its
 * label with a one-line hint under it. `disabled`: why it cannot be used
 * now, shown in place of the hint; `danger`: a destructive one; `attrs`:
 * the page's own attributes; `mark`: the page marks the drawn entry (its
 * Live view control), on every draw.
 * @typedef {{label: string, hint: string, href?: string, download?: string,
 *   onClick?: () => void, disabled?: string | null, danger?: boolean,
 *   attrs?: Record<string, string | null>,
 *   mark?: (e: HTMLElement) => void}} MenuEntry
 * @typedef {{group: string, items: MenuEntry[]}} MenuGroup
 */

/**
 * A header's more menu: kp-themes' menu button (js/menu-button.js, the
 * rich menu ported from this dashboard's More ▾), its entries flat
 * (`items`) or under small headings (`groups`, the stack hub's). The
 * button is the `···` icon marked as the Live view control `drive`, or
 * the page's own (`button`: its text, class, key and mark). kp does the
 * rest: it opens on the first usable entry, closes on a second click,
 * Escape (the focus back on the button), a click outside or a pick, with
 * the theme's own open and close; ↑ ↓ Home End and a first letter move;
 * the menu stays whole on the screen; an entry that cannot be used now
 * stays reachable and says why. `fill` redraws only when what the entries
 * show changed, and never while the menu is open (the change waits until
 * it closes). A closed menu's entries stay in the DOM, so a Live view step
 * aimed at one marks the menu's button (its wrap's first button).
 * @param {{label: string, items?: MenuEntry[], groups?: MenuGroup[],
 *   drive?: Drive, button?: {text: string, class?: string, keys?: string,
 *   mark?: (b: HTMLElement) => void}}} spec
 * @returns {{el: HTMLElement, button: HTMLElement, open: () => void,
 *   close: (focus?: boolean) => void, toggle: () => void,
 *   fill: (groups: MenuGroup[]) => void, stop: () => void}}
 */
export function moreMenu(spec) {
  const list = h("div", {
    class: "kp-menu kp-menu--rich nx-menu",
    role: "menu",
    "aria-label": spec.label,
  });
  const btn = h(
    "button",
    {
      type: "button",
      class: spec.button?.class ?? "nx-icon-btn",
      "aria-label": spec.button ? null : spec.label,
      "aria-keyshortcuts": spec.button?.keys ?? null,
      title: spec.label,
    },
    spec.button?.text ?? "···",
  );
  if (spec.drive) drive(btn, spec.drive);
  spec.button?.mark?.(btn);
  const wrap = h(
    "div",
    { class: "kp-menu-button nx-more", "data-kp-menu-button": "" },
    btn,
    list,
  );
  /** The entries drawn now, by their `data-kp-value`. @type {Map<string, MenuEntry>} */
  let byValue = new Map();
  // kp wires the menu buttons under a root: this one, under a holder it
  // leaves as soon as the page puts it in place.
  attachMenuButtons(h("div", null, wrap), {
    decorate: (part, info) => {
      if (info.kind === "menu-item" && info.value != null)
        byValue.get(info.value)?.mark?.(part);
    },
  });
  wrap.addEventListener(MENU_SELECT_EVENT, (e) => {
    const value = /** @type {CustomEvent<{value: string}>} */ (e).detail.value;
    byValue.get(value)?.onClick?.();
  });
  /** @param {MenuGroup[]} groups */
  const fill = (groups) => {
    /** @type {Map<string, MenuEntry>} */
    const next = new Map();
    const kp = groups.map((g, gi) => ({
      group: g.group,
      items: g.items.map((it, ii) => {
        const value = `${gi}.${ii}`;
        next.set(value, it);
        return {
          label: it.label,
          hint: it.hint,
          value,
          ...(it.href ? { href: it.href } : {}),
          ...(it.download ? { download: it.download } : {}),
          ...(it.disabled ? { disabled: it.disabled } : {}),
          ...(it.danger ? { danger: true } : {}),
          ...(it.attrs ? { attrs: it.attrs } : {}),
        };
      }),
    }));
    byValue = next;
    setMenu(wrap, kp);
  };
  fill(spec.groups ?? [{ group: "", items: spec.items ?? [] }]);
  const isOpen = () => btn.getAttribute("aria-expanded") === "true";
  return {
    el: wrap,
    button: btn,
    open: () => openMenu(wrap),
    close: (focus = false) => closeMenu(wrap, { focus }),
    toggle: () => (isOpen() ? closeMenu(wrap) : openMenu(wrap)),
    fill,
    stop: () => closeMenu(wrap),
  };
}

/**
 * A keyboard key, as printed beside the action it triggers.
 * @param {string} k
 */
export const kbd = (k) => h("kbd", { class: "kp-kbd" }, k);

/**
 * The page's compact line of shortcuts ("/ search containers · …"), kp's
 * `.kp-keys`. A key may be several keys for one action (`[["↑", "↓"],
 * "move"]`).
 * @param {[string | string[], string][]} pairs keys, what they do
 */
export const keyRow = (pairs) => {
  const el = h("p");
  setKeys(el, pairs);
  return el;
};

/** The same block under its first name. */
export const keysLine = keyRow;

/**
 * A stack's colour from the topology's hues (`stackHues`), readable on
 * light and dark. The one place outside the stack mark where a page
 * computes a colour instead of reading a kp-themes token: a stack's hue
 * is its identity (DESIGN_LANGUAGE §11), and no token holds a colour per
 * stack.
 * @param {number} hue
 */
export const hueColour = (hue) =>
  `light-dark(hsl(${hue} 65% 42%), hsl(${hue} 70% 64%))`;

/**
 * A stack's colour by its place in the fleet's sorted names: one of the
 * five chart colours; a name the fleet does not list is neutral, never
 * mistaken for the first stack.
 * @param {string} name
 * @param {string[]} all
 */
export const chartColour = (name, all) => {
  const i = [...all].sort().indexOf(name);
  return i < 0 ? NEUTRAL_COLOUR : `var(--chart-${(i % 5) + 1})`;
};

/** The colour of a name the fleet does not list: no chart colour of its own. */
export const NEUTRAL_COLOUR = "var(--muted-foreground)";

/**
 * The small square of a stack's colour beside its name.
 * @param {string} colour a CSS colour (`hueColour`, `chartColour`)
 */
export function swatch(colour) {
  const s = h("span", { class: "nx-swatch", "aria-hidden": "true" });
  s.style.setProperty("--c", colour);
  return s;
}

/** Load a page's own stylesheet once (dom.js holds the one loader). */
export { ensureStyle } from "./dom.js";

/** @typedef {"ok" | "warn" | "bad" | "info" | "live" | ""} Tone */

/** kp-themes' badge variant for a dashboard tone. @type {Record<string, string>} */
const BADGE_TONE = {
  ok: "success",
  warn: "warning",
  bad: "destructive",
  info: "info",
  live: "info",
};

/**
 * A small status chip, kp-themes' `.kp-badge`: a count, a state word ("3",
 * "unreadable"), on its status plate (`--success` and the like, each with
 * its own ink) or plain. With `dot`, a status dot leads the word.
 * @param {import("./dom.js").Child} text
 * @param {{tone?: Tone, dot?: boolean, title?: string, label?: string}} [o]
 *   `label`: what a screen reader says instead of the text ("!")
 */
export function chip(text, o = {}) {
  const variant = o.tone ? BADGE_TONE[o.tone] : undefined;
  return h(
    "span",
    {
      class: `kp-badge${variant ? ` kp-badge--${variant}` : ""}`,
      // A page's own tone (Claude's chip) has no kp variant: a hook only.
      "data-tone": o.tone && !variant ? o.tone : null,
      title: o.title ?? null,
      "aria-label": o.label ?? null,
    },
    o.dot
      ? h("span", { class: "kp-health__dot", "aria-hidden": "true" })
      : null,
    text,
  );
}

/**
 * A status dot, kp-themes' `.kp-health` with its word beside it (the word
 * is what a screen reader reads; a dot without one is decorative). kp
 * knows ok / warn / down; `info` and `live` are the dashboard's own two
 * (see app.css).
 * @param {Tone} tone
 * @param {import("./dom.js").Child} [word]
 * @param {{cls?: string, attrs?: Record<string, string>}} [o] further
 *   classes and attributes of the element
 */
export function dot(tone, word, o = {}) {
  const state = {
    ok: "ok",
    warn: "warn",
    bad: "down",
    info: "info",
    live: "ok",
  }[/** @type {"ok" | "warn" | "bad" | "info" | "live"} */ (tone)];
  return h(
    "span",
    {
      class: o.cls ? `kp-health ${o.cls}` : "kp-health",
      "data-state": state ?? null,
      "aria-hidden": word ? null : "true",
      ...(o.attrs ?? {}),
    },
    h("span", { class: "kp-health__dot", "aria-hidden": "true" }),
    word ?? null,
  );
}

/**
 * A meter on its own (a KPI tile's, a table cell's), kp-themes'
 * `.kp-meter`: a bar `pct` full, with a tick at `mark` and the warn / bad
 * tone (`setMeter` writes the share, the mark and the ARIA).
 * @param {Meter} m
 * @param {string} [cls] the element's classes (`kp-meter`)
 * @returns {{el: HTMLElement, set: (m: Meter) => void}}
 */
export function meter(m, cls = "kp-meter") {
  const e = h("span", { class: cls });
  const set = (/** @type {Meter} */ next) => paintMeter(e, next);
  set(m);
  return { el: e, set };
}

/**
 * The error state of a block (DESIGN_LANGUAGE §7: loading, empty, error
 * and filled take the same footprint): what could not be read, why, the
 * fix, and Try again (the Live view control `retry.drive`).
 * @param {{what: string, why: string, fix?: string | null}} err
 * @param {{run: () => void, drive: Drive, label?: string} | null} [retry]
 */
export function failBox(err, retry = null) {
  const plate = statePlate(document, {
    kind: "error",
    title: err.what,
    text: [err.why, ...(err.fix ? [`Fix: ${err.fix}`] : [])].join("\n"),
    ...(retry
      ? {
          actionLabel: retry.label ?? "Try again",
          onAction: retry.run,
        }
      : {}),
  });
  plate.classList.add("nx-fail");
  plate.setAttribute("data-kp-state", "error");
  const again = plate.querySelector(".kp-stateplate__action");
  if (retry && again instanceof HTMLElement) {
    again.title = "Read it again";
    drive(again, retry.drive);
  }
  return plate;
}

/**
 * A side panel (DESIGN_LANGUAGE §6) as kp-themes draws one
 * (`dialog.kp-drawer`): a modal `<dialog>` against the end edge (the whole
 * width on a phone) with a head (title, one sentence, Close), a body that
 * scrolls and a foot of actions. Escape, Close and a click on the backdrop
 * close it; its buttons are found by Live view among the open dialog's own
 * (Close is the dialog control `close`).
 * @param {{title: string, desc: string, body?: import("./dom.js").Child,
 *   foot?: import("./dom.js").Child, onClose?: () => void,
 *   cls?: string}} spec
 * @returns {{el: HTMLDialogElement, body: HTMLElement, foot: HTMLElement,
 *   open: () => void, close: () => void}}
 */
export function drawer(spec) {
  const id = `nx-drawer-${++drawers}`;
  const body = h("div", { class: "kp-drawer__body" }, spec.body ?? null);
  const foot = h("footer", { class: "kp-drawer__foot" }, spec.foot ?? null);
  foot.hidden = spec.foot == null;
  const close = () => {
    if (d.open) d.close();
  };
  const x = dialogControl(
    h(
      "button",
      {
        type: "button",
        // kp-dialog__close: the same ✕ every dialog of the app has.
        class: "kp-icon-button kp-dialog__close",
        "aria-label": "Close",
        title: "Close (Esc)",
        onclick: close,
      },
      "✕",
    ),
    "close",
  );
  const d = /** @type {HTMLDialogElement} */ (
    h(
      "dialog",
      {
        class: `kp-drawer kp-dialog${spec.cls ? ` ${spec.cls}` : ""}`,
        "aria-labelledby": `${id}-h`,
      },
      h(
        "header",
        { class: "kp-drawer__head" },
        h("h2", { id: `${id}-h`, class: "kp-dialog__title" }, spec.title),
        x,
        h("p", { class: "kp-drawer__desc" }, spec.desc),
      ),
      body,
      foot,
    )
  );
  d.addEventListener("click", (e) => {
    if (e.target === d) close();
  });
  d.addEventListener("close", () => {
    d.remove();
    spec.onClose?.();
  });
  return {
    el: d,
    body,
    foot,
    open: () => {
      if (!d.isConnected) document.body.append(d);
      if (!d.open) d.showModal();
    },
    close,
  };
}
let drawers = 0;

/**
 * The phone breakpoint every shared block switches at: one value, the
 * same as app.css's `@media (max-width: 48rem)` (CSS cannot read a custom
 * property inside a media query, so the guard test
 * `uikit.test.js` holds the stylesheet to this one number).
 */
export const PHONE = "(max-width: 48rem)";

/**
 * A kp datatable with no search bar of its own: sortable headers,
 * a click adds a further key (data-kp-sort-multi="click"), the sort remembered under `remember`; the
 * caller fills `tbody`, then wires it with a `tableSlot` (or
 * `attachDataTables`). `cls` adds the page's own class (Metrics, the Map).
 * @param {{remember: string, caption: string, cls?: string,
 *   columns: {label: string, sort?: string, cls?: string}[]}} spec
 */
export function dataTable(spec) {
  const tbody = h("tbody");
  const wrap = h(
    "div",
    {
      class: `kp-datatable${spec.cls ? ` ${spec.cls}` : ""}`,
      "data-kp-datatable": "",
      "data-kp-sort-multi": "click",
      "data-kp-remember": spec.remember,
      "data-kp-page-sizes": "none",
      "data-kp-page-size": "500",
    },
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h("caption", { class: "kp-sr-only" }, spec.caption),
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            ...spec.columns.map((c) =>
              h(
                "th",
                {
                  ...(c.sort && c.sort !== "none"
                    ? { "data-kp-sort": c.sort }
                    : {}),
                  ...(c.cls ? { class: c.cls } : {}),
                },
                c.label,
              ),
            ),
          ),
        ),
        tbody,
      ),
    ),
  );
  return { wrap, tbody };
}

/**
 * One card's datatable behaviour: `attach(body)` wires the table just put
 * in `body` and stops the one it replaced, so a page that repaints every
 * 30 s keeps one listener set per card, not one per repaint.
 * @param {(root: HTMLElement) => () => void} [attachFn] attachDataTables
 *   (a test passes its own)
 */
export function tableSlot(attachFn = attachDataTables) {
  /** @type {(() => void) | null} */
  let stop = null;
  return {
    /** @param {HTMLElement} body */
    attach: (body) => {
      stop?.();
      stop = attachFn(body);
    },
    stop: () => {
      stop?.();
      stop = null;
    },
  };
}

/**
 * Whether a focused element keeps j / k / Enter to itself (a field, a
 * button, a link); anchored, so a row, a card or a span never does
 * (redesign-config-12).
 * @param {string} tag
 */
export const typingTarget = (tag) =>
  /^(INPUT|TEXTAREA|SELECT|BUTTON|A)$/.test(tag);

/**
 * The text with the query's first match marked (case-insensitive), as ONE
 * element: a chip is a flex box, and loose text nodes beside a `<mark>`
 * would each become a flex item with the chip's gap between them
 * ("c a dvisor", redesign-config-4).
 * @param {string | null | undefined} text
 * @param {string} q lower-case
 * @returns {import("./dom.js").Child}
 */
export function highlight(text, q) {
  if (!text) return text ?? "";
  if (!q) return text;
  const i = text.toLowerCase().indexOf(q);
  if (i < 0) return text;
  return h(
    "span",
    { class: "nx-hl" },
    text.slice(0, i),
    h("mark", { class: "nx-mark" }, text.slice(i, i + q.length)),
    text.slice(i + q.length),
  );
}

/**
 * One alert for every read a page lost at once (redesign-config-11): the
 * cause once, under the header, with ONE Try again (the Live view control
 * `drive`); the blocks only say they are empty. Hidden while
 * nothing failed.
 * @param {{drive: Drive}} spec
 */
export function failBand(spec) {
  const box = h("div", { class: "nx-failband" });
  box.hidden = true;
  return {
    el: box,
    /**
     * @param {{title: string, why: string, fix?: string | null} | null} f
     * @param {() => void} retry
     */
    set: (f, retry) => {
      box.hidden = !f;
      if (!f) return box.replaceChildren();
      const plate = statePlate(document, {
        kind: "error",
        title: f.title,
        text: [
          `Why: ${f.why}`,
          ...(f.fix ? [`What to do: ${f.fix}`] : []),
        ].join("\n"),
        actionLabel: "Try again",
        onAction: retry,
      });
      plate.classList.add("nx-fail");
      const again = plate.querySelector(".kp-stateplate__action");
      if (again instanceof HTMLElement) {
        again.title = "Read it again";
        drive(again, spec.drive);
      }
      box.replaceChildren(plate);
    },
  };
}

/**
 * An SVG line drawing from a list of shapes, 120×72, in currentColor.
 * @param {string} label for the test hooks; the drawing itself is hidden
 * @param {[string, Record<string, string>][]} shapes
 * @returns {SVGSVGElement}
 */
function drawing(label, shapes) {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", "nx-art");
  svg.setAttribute("viewBox", "0 0 120 72");
  svg.setAttribute("width", "120");
  svg.setAttribute("height", "72");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.6");
  svg.setAttribute("aria-hidden", "true");
  svg.dataset.art = label;
  for (const [tag, attrs] of shapes) {
    const e = document.createElementNS(SVG, tag);
    for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, v);
    svg.append(e);
  }
  return svg;
}

/** The empty and all-clear drawings of graphics.html (approved). */
export const art = {
  /** Nothing matches a filter: a magnifier over an empty line. */
  noMatch: () =>
    drawing("no-match", [
      ["circle", { cx: "52", cy: "32", r: "18" }],
      [
        "path",
        { d: "M65 45l17 17", "stroke-width": "3", "stroke-linecap": "round" },
      ],
      ["path", { d: "M44 32h16", opacity: ".5", "stroke-linecap": "round" }],
    ]),
  /** All clear: a shield with a tick, in the success colour. */
  allClear: () =>
    drawing("all-clear", [
      [
        "path",
        {
          d: "M60 8l30 11v17c0 16-13 25-30 30C43 61 30 52 30 36V19z",
          class: "nx-art__ok-fill",
        },
      ],
      [
        "path",
        {
          d: "M47 37l9 9 18-19",
          class: "nx-art__ok",
          "stroke-width": "3",
          "stroke-linecap": "round",
          "stroke-linejoin": "round",
        },
      ],
    ]),
  /** No stacks yet: an empty container with a plus. */
  noStacks: () =>
    drawing("no-stacks", [
      ["rect", { x: "22", y: "14", width: "76", height: "46", rx: "6" }],
      ["path", { d: "M22 26h76", opacity: ".5" }],
      [
        "path",
        { d: "M34 38h30M34 47h20", opacity: ".35", "stroke-linecap": "round" },
      ],
      ["circle", { cx: "88", cy: "50", r: "13", class: "nx-art__accent" }],
      [
        "path",
        {
          d: "M88 44v12M82 50h12",
          class: "nx-art__accent-line",
          "stroke-linecap": "round",
        },
      ],
    ]),
  /** No tiles yet: an empty grid of four tiles, one dashed. */
  noTiles: () =>
    drawing("no-tiles", [
      ["rect", { x: "26", y: "10", width: "30", height: "22", rx: "5" }],
      ["rect", { x: "64", y: "10", width: "30", height: "22", rx: "5" }],
      ["rect", { x: "26", y: "40", width: "30", height: "22", rx: "5" }],
      [
        "rect",
        {
          x: "64",
          y: "40",
          width: "30",
          height: "22",
          rx: "5",
          "stroke-dasharray": "4 4",
          class: "nx-art__accent-line",
        },
      ],
    ]),
};

/**
 * @typedef {{title: string, desc: string, state: "wait" | "run" | "ok" |
 *   "bad" | "skip", time?: string, note?: string}} CheckRow
 */

/** The big tick of a finished flow or an empty Inbox. */
export function doneMark(tone = "ok") {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 64 64");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2.4");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("nx-done__mark", `nx-done__mark--${tone}`);
  const c = document.createElementNS(ns, "circle");
  c.setAttribute("cx", "32");
  c.setAttribute("cy", "32");
  c.setAttribute("r", "27");
  c.setAttribute("opacity", ".25");
  const p = document.createElementNS(ns, "path");
  p.setAttribute(
    "d",
    tone === "ok" ? "m20 33 8 8 16-18" : "M24 24l16 16M40 24 24 40",
  );
  svg.append(c, p);
  return svg;
}
