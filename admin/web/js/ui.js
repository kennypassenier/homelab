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
//   toolbar      §12: search · labelled filter groups · view & state, plus
//                the active-filter row of filterChips
//   toggleGroup  the plain-click toggle group (several on, every one on is
//                All, Esc / All / Show all resets), segmented or as chips
//                (`toggleChips`); segSwitch the one-of switch
//   section      a card: heading + one sentence + tools, optionally
//                collapsible and mounted only when first opened, with a
//                foot line (source left, "read 4 s ago" right)
//   sortHead / sortableTable / nextSort / applySort / rememberedSort
//                sorting on several keys, remembered per table
//   toast        kp-themes' toast in a region inside #page, so Live view
//                can press its Undo
//   rowMenu / moreMenu   a row's menu (a non-modal <dialog>) and the
//                header's overflow `···` menu, both keyboard menus
//   drawer       a side panel on a modal <dialog>
//   hoverCard    one floating detail card for hover and keyboard focus
//   chip / dot / meter / shareBar / swatch / stackMark   small marks
//   failBox / skeleton… / emptyState   the error, loading and empty states
//                in the block's own footprint
//   rowKeys / kbd / keyRow   keyboard: list rows, a key, the shortcut line
//   sparkline    a 28 px trend line for a tile or a table row
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
// The shared time chart lives in timechart.js.

import { agoEl, setAgo } from "./ago.js";
import { h } from "./dom.js";
import { dialogControl, drivable } from "./drivable.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import { toast as kpToast } from "/static/kp/js/overlays.js";
import { applySort, keepSort, nextSort, rememberedSort } from "./sortstate.js";

export {
  applySort,
  compareValues,
  keepSort,
  nextSort,
  rememberedSort,
} from "./sortstate.js";

/** @typedef {import("./sortstate.js").SortKey} SortKey */

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
 * The live freshness of a page or a card: a dot and "updated 4 s ago",
 * ticking (ago.js), greyed out once it is older than three minutes.
 * `set(at, words)`: `words` stand in for the time while something is under
 * way ("reading 3 of 6 stacks…"); `data-state` says none / ok / busy.
 * @param {string} [verb] "updated", "read", "measured"
 * `fail(words)`: the read failed ("not read"), a red dot.
 * @returns {{el: HTMLElement,
 *   set: (at: number | null, words?: string) => void,
 *   fail: (words: string) => void}}
 */
export function liveStatus(verb = "updated") {
  const ago = agoEl(verb, null, { live: true });
  const words = h("span", { class: "nx-live__words" });
  const e = h("span", { class: "nx-live", role: "status" }, ago, words);
  /** @param {number | null} at @param {string} [w] */
  const set = (at, w) => {
    setAgo(ago, at);
    ago.hidden = w != null;
    words.hidden = w == null;
    words.textContent = w ?? "";
    e.dataset.state = w != null ? "busy" : at == null ? "none" : "ok";
  };
  set(null);
  /** @param {string} w */
  const fail = (w) => {
    set(null, w);
    e.dataset.state = "failed";
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
 *   `split`: the next.css header (Secrets): the live status beside the
 *   title, the description under them, the actions against the right
 *   edge spanning both rows (one column on a phone, actions last);
 *   `level`: the title's heading level, `h1` by default; `h2` when the
 *   page is embedded under another page's title (Schedules as Activity's
 *   "Planned" view);
 *   `live`: show a live status (true, or the verb: "updated", "read");
 *   `actions`: at most two secondary actions;
 *   `primary`: the ONE primary action, after them;
 *   `more`: the overflow menu (`moreMenu`), last;
 *   `meta`: chips and facts in a row under the description (the live
 *   status joins them); the header then takes the demo's grid, its
 *   actions against the right edge spanning the title, description and
 *   meta rows (one column on a phone, actions last).
 */

/**
 * The page header (DESIGN_LANGUAGE §1.1): breadcrumbs, then one title row
 * (title left; live status and the actions grouped against the right
 * edge, the primary one last), then the page's one-sentence description
 * right under it. Markup the whole-screen invariants read: `.title-row`
 * holding the `h1`, the description its next sibling `p` (rows 32, 36, 40,
 * 41); with `meta`, the description is the `h1`'s own next sibling.
 * @param {HeaderSpec} spec
 * @returns {{el: HTMLElement, title: HTMLHeadingElement,
 *   desc: HTMLParagraphElement, actions: HTMLElement,
 *   meta: HTMLElement | null,
 *   live: ReturnType<typeof liveStatus> | null}}
 */
export function pageHeader(spec) {
  const title = h(
    spec.level ?? "h1",
    null,
    spec.title,
    ...(spec.sub ? [h("span", { class: "nx-head-sub" }, ` ${spec.sub}`)] : []),
  );
  const live =
    spec.live === undefined || spec.live === false
      ? null
      : liveStatus(typeof spec.live === "string" ? spec.live : "updated");
  const actions = h(
    "div",
    { class: "actions-row nx-head-actions" },
    ...(spec.actions ?? []),
    ...(spec.primary ? [spec.primary] : []),
    ...(spec.more ? [spec.more] : []),
  );
  const desc = h("p", { class: "section-head__desc nx-head-desc" }, spec.desc);
  const crumbs = spec.crumbs?.length ? [breadcrumbs(spec.crumbs)] : [];
  if (spec.meta) {
    const meta = h(
      "div",
      { class: "nx-head-meta" },
      ...spec.meta,
      ...(live ? [live.el] : []),
    );
    const e = h(
      "header",
      { class: "nx-head nx-head--meta" },
      ...crumbs,
      title,
      desc,
      meta,
      actions,
    );
    return { el: e, title, desc, actions, meta, live };
  }
  if (spec.split) {
    const e = h(
      "header",
      { class: "nx-head nx-head--split" },
      ...crumbs,
      h("div", { class: "title-row" }, title, live?.el ?? null),
      desc,
      actions,
    );
    return { el: e, title, desc, actions, meta: null, live };
  }
  const titleMeta = spec.titleMeta
    ? h("div", { class: "nx-head-titlemeta" }, ...spec.titleMeta)
    : null;
  const row = h("div", { class: "title-row" }, title, titleMeta);
  if (live || actions.childElementCount > 0) {
    const right = h("div", { class: "nx-head-right" });
    if (live) right.append(live.el);
    right.append(actions);
    row.append(right);
  }
  const e = h("header", { class: "nx-head" }, ...crumbs, row, desc);
  return { el: e, title, desc, actions, meta: titleMeta, live };
}

/**
 * @typedef {{pct: number | null, mark?: number | null, markTitle?: string,
 *   tone?: "" | "warn" | "bad" | null}} Meter
 *   a bar `pct` percent full, with a tick at `mark` percent (what is
 *   promised, a limit), in the warn / bad colour when `tone` says so
 */

/**
 * @typedef {{key?: string, label: string, value?: string, unit?: string,
 *   ctx?: string, ctxTone?: "ok" | "warn" | "bad" | "" | null,
 *   ctxParts?: {text: string, tone?: "ok" | "warn" | "bad" | ""}[] | null,
 *   colour?: string, target?: string,
 *   tone?: "" | "warn" | "bad" | null, href?: string,
 *   spark?: number[], meter?: Meter | null, title?: string,
 *   toggle?: {pressed: boolean, onToggle: () => void, drive?: Drive}}} Kpi
 *   `spark` or `meter` fill the tile's fourth row (a tile given neither
 *   has three rows). `href`: the tile links to its filtered detail;
 *   `toggle`: the tile is a button turning a filter on this page on or
 *   off (`aria-pressed`), the Live view control `drive` (required when the
 *   tile is made; a later `set` may leave it out). `ctxTone` puts a
 *   status dot before the context line; `ctxParts` draws the context as
 *   pieces, each with its own dot ("7 enforced" green, "2 open" amber).
 *   `colour`: the sparkline's colour. `target`: the id of the card on this
 *   page the tile sums up; the tile is a same-site link to it and scrolls
 *   there (Metrics, the Map). A tile made without a spark or meter grows
 *   its fourth row when a later `set` brings one.
 */

/**
 * A meter's geometry, clamped to its bar.
 * @param {Meter | null | undefined} m
 * @returns {{fill: number, mark: number | null, tone: string}}
 */
export function meterParts(m) {
  const clamp = (/** @type {number} */ n) =>
    Math.max(0, Math.min(100, Number.isFinite(n) ? n : 0));
  return {
    fill: clamp(m?.pct ?? 0),
    mark: m?.mark == null ? null : clamp(m.mark),
    tone: m?.tone ?? "",
  };
}

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
 * One KPI tile (DESIGN_LANGUAGE §1.2): label (xs uppercase), value
 * (tabular), a context line, an optional sparkline or meter; a tile with
 * an `href` is a link to its filtered detail, one with a `toggle` a
 * button. `set` repaints it in place, so the strip never reflows (rule 6);
 * `set({loading: true})` shows skeletons in the value and the context.
 * @param {Kpi} k
 * @returns {{el: HTMLElement,
 *   set: (k: Partial<Kpi> & {loading?: boolean}) => void}}
 */
export function kpi(k) {
  const label = h("span", { class: "nx-kpi__label" });
  const hint = h("span", { class: "nx-kpi__hint", "aria-hidden": "true" });
  const value = h("span", { class: "nx-kpi__value" });
  const ctx = h("span", { class: "nx-kpi__ctx" });
  let m = k.meter ? meter(k.meter) : null;
  /** @type {HTMLElement | null} */
  let foot = m
    ? m.el
    : k.spark
      ? h("span", { class: "nx-kpi__spark", "aria-hidden": "true" })
      : null;
  // A same-site path (invariant 35), the card's id as its hash.
  const href = k.target
    ? `${location.pathname}${location.search}#${k.target}`
    : k.href;
  /** @type {HTMLElement} */
  const e = k.toggle
    ? h("button", { type: "button", class: "nx-kpi nx-kpi--toggle" })
    : h(href ? "a" : "div", {
        class: "nx-kpi",
        ...(href ? { href } : {}),
      });
  if (!foot) e.classList.add("nx-kpi--bare");
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
    e.addEventListener("click", () => cur.toggle?.onToggle());
  }
  let loading = false;
  const set = (/** @type {Partial<Kpi> & {loading?: boolean}} */ next) => {
    const { loading: l, ...rest } = next;
    if (l != null) loading = l;
    cur = { ...cur, ...rest };
    label.textContent = cur.label;
    if (loading) e.dataset.loading = "";
    else delete e.dataset.loading;
    value.replaceChildren(
      ...(loading
        ? [skeleton("3ch")]
        : [
            cur.value ?? "—",
            ...(cur.unit ? [h("small", null, cur.unit)] : []),
          ]),
    );
    ctx.replaceChildren(
      ...(loading
        ? [skeleton("80%")]
        : cur.ctxParts?.length
          ? cur.ctxParts.flatMap((p, i) => [
              ...(i ? [" · "] : []),
              h(
                "span",
                p.tone ? { class: `nx-dot nx-dot--${p.tone}` } : null,
                p.text,
              ),
            ])
          : [
              cur.ctxTone
                ? h(
                    "span",
                    { class: `nx-dot nx-dot--${cur.ctxTone}` },
                    cur.ctx ?? "",
                  )
                : (cur.ctx ?? ""),
            ]),
    );
    e.dataset.tone = cur.tone ?? "";
    e.dataset.key = cur.key ?? cur.label;
    if (cur.title) e.title = cur.title;
    if (cur.toggle) {
      e.setAttribute("aria-pressed", String(cur.toggle.pressed));
      hint.textContent = cur.toggle.pressed ? "filtering ×" : "filter";
    }
    if (!foot && (cur.meter || (cur.spark && cur.spark.length > 1))) {
      foot = h("span", { class: "nx-kpi__spark", "aria-hidden": "true" });
      e.classList.remove("nx-kpi--bare");
      e.append(foot);
    }
    if (cur.meter) {
      if (m) m.set(cur.meter);
      else if (foot) {
        m = meter(cur.meter);
        foot.replaceChildren(m.el);
      }
    } else if (foot) {
      if (m && foot !== m.el) m = null;
      if (!m)
        foot.replaceChildren(
          ...(cur.spark && cur.spark.length > 1
            ? [sparkline(cur.spark, cur.colour ? { colour: cur.colour } : {})]
            : []),
        );
    }
    if (cur.href && e instanceof HTMLAnchorElement) e.href = cur.href;
  };
  set({});
  return { el: e, set };
}

/**
 * A row of 3 to 6 KPI tiles; while `loading` each tile is its own skeleton
 * in the final geometry.
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
    class: "nx-kpis",
    role: "group",
    "aria-label": opts.label ?? "Key figures",
  });
  e.style.setProperty("--kpi-n", String(Math.max(1, tiles.length)));
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
 *   clear: () => void, drive: Drive}[],
 *   clearAll?: {run: () => void, drive: Drive}) => void}}
 *   `setActive`: one `filterChip` per active filter, each the Live view
 *   control its `drive` names, and Clear all (its own control)
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
        ...chips.map((c) => filterChip(c.label, c.clear, c.drive)),
        ...(clearAll
          ? [
              drive(
                h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--ghost kp-button--sm",
                    title: "Turn every filter off",
                    onclick: clearAll.run,
                  },
                  "Clear all",
                ),
                clearAll.drive,
              ),
            ]
          : []),
      );
      active.hidden = chips.length === 0;
    },
  };
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
          class: cls(" nx-card--fold"),
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
    stop: () => cleanup?.(),
    open: () => {
      if (e instanceof HTMLDetailsElement) e.open = true;
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
 *   all?: {label: string, hint?: string} | null,
 *   look?: "seg" | "chips"}} ToggleSpec
 *   `drive`: the Live view control every button is, on its own value's
 *   row ("all" for All) — required, a toggle nobody can press from Live
 *   view is a defect (invariant 39). `all`: an "All" button first, pressed
 *   while nothing is on. `look`: "seg" (the segmented control, the
 *   default) or "chips" (labelled chips in a toolbar group).
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
  const chipsLook = spec.look === "chips";
  /** @type {Chip[]} */
  let chips = spec.chips;
  /** @type {Set<string>} */
  let on = new Set(spec.selected ?? []);
  if (on.size >= chips.length) on = new Set();
  /** @type {Record<string, number | null | undefined>} */
  let n = Object.fromEntries(chips.map((c) => [c.value, c.count]));
  const box = h("span", { class: chipsLook ? "nx-chips" : "nx-seg__btns" });
  const e = chipsLook
    ? h(
        "div",
        { class: "nx-tb__group", role: "group", "aria-label": spec.label },
        h("b", null, spec.label),
        box,
      )
    : h("div", { class: "nx-seg", role: "group", "aria-label": spec.label });
  const changed = () => {
    paint();
    spec.onChange(new Set(on));
  };
  /**
   * @param {string} value @param {string} label @param {string} title
   * @param {() => void} click
   */
  const btn = (value, label, title, click) => {
    const c = n[value];
    return drive(
      h(
        "button",
        {
          type: "button",
          class: chipsLook ? "nx-chip-toggle" : null,
          "data-v": value,
          "aria-pressed": String(
            value === "all" ? on.size === 0 : on.has(value),
          ),
          title,
          onclick: click,
        },
        label,
        h(
          "span",
          { class: chipsLook ? "nx-chip-toggle__count" : "nx-seg__count" },
          c == null ? "" : String(c),
        ),
      ),
      { id: spec.drive.id, row: value },
    );
  };
  const paint = () => {
    const btns = [
      ...(spec.all
        ? [
            btn(
              "all",
              spec.all.label,
              spec.all.hint ?? "Show every row",
              () => {
                on = new Set();
                changed();
              },
            ),
          ]
        : []),
      ...chips.map((c) =>
        btn(
          c.value,
          c.label,
          c.hint ?? `Show or hide ${c.label}; each click turns it on or off`,
          () => {
            on = toggleValue(on, c.value, chips.length);
            changed();
          },
        ),
      ),
    ];
    const focused = e.querySelector(":focus");
    const v = focused instanceof HTMLElement ? focused.dataset.v : null;
    (chipsLook ? box : e).replaceChildren(...btns);
    if (v != null)
      /** @type {HTMLElement | undefined} */ (
        btns.find((b) => b.dataset.v === v)
      )?.focus();
  };
  paint();
  return {
    el: e,
    selected: () => new Set(on),
    set: (values) => {
      on = new Set(values);
      if (on.size >= chips.length) on = new Set();
      paint();
    },
    reset: () => {
      if (on.size === 0) return;
      on = new Set();
      changed();
    },
    counts: (next) => {
      n = { ...n, ...next };
      paint();
    },
    setChips: (next) => {
      chips = next;
      for (const c of chips) if (c.count !== undefined) n[c.value] = c.count;
      for (const v of [...on])
        if (!chips.some((c) => c.value === v)) on.delete(v);
      paint();
    },
  };
}

/**
 * The same toggle group in the chips look, for a toolbar's filter groups.
 * @param {Omit<ToggleSpec, "look">} spec
 */
export const toggleChips = (spec) => toggleGroup({ ...spec, look: "chips" });

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
  /** @type {Map<string, HTMLElement>} */
  const counts = new Map();
  let cur = spec.value;
  const btns = spec.items.map((it) => {
    const c = h("span", { class: "nx-seg__count" });
    counts.set(it.value, c);
    const b = h(
      "button",
      {
        type: "button",
        "data-v": it.value,
        title: it.hint ?? null,
        onclick: () => {
          if (cur === it.value) return;
          cur = it.value;
          paint();
          spec.onChange(cur);
        },
      },
      it.label,
      c,
    );
    if (spec.drive) drive(b, { id: spec.drive.id, row: it.value });
    else spec.mark?.(b, it.value);
    return b;
  });
  const paint = () =>
    btns.forEach((b, i) =>
      b.setAttribute("aria-pressed", String(spec.items[i].value === cur)),
    );
  paint();
  return {
    el: h(
      "div",
      { class: "nx-seg", role: "group", "aria-label": spec.label },
      ...btns,
    ),
    set: (v) => {
      cur = v;
      paint();
    },
    value: () => cur,
    counts: (n) => {
      for (const [v, c] of counts)
        c.textContent = n[v] == null ? "" : String(n[v]);
    },
  };
}

/**
 * What a cell sorts by: its `data-sort` or its text, numbers as numbers.
 * @param {string} v
 * @returns {number | string}
 */
export function sortValue(v) {
  const n = Number(v);
  return v !== "" && Number.isFinite(n) ? n : v.toLowerCase();
}

/** The mark a sorted column shows: ↑ / ↓, numbered when several keys sort. */
export const sortMark = (
  /** @type {SortKey[]} */ sort,
  /** @type {string} */ key,
) => {
  const i = sort.findIndex((s) => s.key === key);
  if (i < 0) return "↕";
  return `${sort[i].dir > 0 ? "↑" : "↓"}${sort.length > 1 ? i + 1 : ""}`;
};

/**
 * @typedef {{label: string, key: string, sort: SortKey[],
 *   onSort: (next: SortKey[]) => void, drive: Drive, remember?: string,
 *   cls?: string}} SortHeadSpec
 *   `sort`: the table's sort now; `onSort` gets the next one (plain click:
 *   this column alone, ascending → descending → none; Shift+click adds it
 *   as a further key); `remember`: the table's name, to keep the sort for
 *   the next visit (`rememberedSort` reads it back); `drive`: the Live view
 *   control the header's button is.
 */

/**
 * A sortable column header (DESIGN_LANGUAGE §12; Kenny: multi-sort,
 * remembered per table): a `th` with `aria-sort` holding a button and the
 * column's mark (↕, or ↑ / ↓ numbered when several keys sort).
 * @param {SortHeadSpec} spec
 * @returns {HTMLTableCellElement}
 */
export function sortHead(spec) {
  const s = spec.sort.find((x) => x.key === spec.key);
  const btn = h(
    "button",
    {
      type: "button",
      class: "nx-sort",
      title:
        "Sort: click for ascending, again for descending, a third time for none · Shift+click adds a further sort",
      onclick: (/** @type {MouseEvent} */ e) => {
        const next = nextSort(spec.sort, spec.key, e.shiftKey);
        if (spec.remember) keepSort(spec.remember, next);
        spec.onSort(next);
      },
    },
    spec.label,
    h(
      "span",
      { class: "nx-sort__mark", "aria-hidden": "true" },
      sortMark(spec.sort, spec.key),
    ),
  );
  drive(btn, spec.drive);
  return h(
    "th",
    {
      scope: "col",
      class: spec.cls ?? null,
      "aria-sort": s ? (s.dir > 0 ? "ascending" : "descending") : "none",
    },
    btn,
  );
}

/**
 * The key a plain table's column sorts under: its `data-key`, else its
 * header text in lower-case words ("Memory" → "memory").
 * @param {HTMLTableCellElement} th
 */
const columnKey = (th) =>
  th.dataset.key ??
  (th.textContent ?? "")
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

/**
 * Makes a plain table's headers sort it (DESIGN_LANGUAGE §12) with the
 * same rule as `sortHead`: a click sorts by that column (ascending,
 * descending, the original order), Shift+click adds it as a further key,
 * Enter / Space do the same; `aria-sort` and the header's mark say which.
 * A cell's `data-sort` overrides its text. Each header is the Live view
 * control `drive.id` on its column's row; with `remember` the table keeps
 * its sort for the next visit. Returns `apply()`, which sorts the body
 * again after the page refilled it.
 * @param {HTMLTableElement} table
 * @param {{drive: {id: string}, remember?: string}} opts
 * @returns {{apply: () => void, sort: () => SortKey[]}}
 */
export function sortableTable(table, opts) {
  if (!opts?.drive?.id)
    throw new Error("a sortable table's headers need their Live view control");
  const head = table.tHead?.rows[0];
  /** @type {SortKey[]} */
  let sort = opts.remember ? rememberedSort(opts.remember) : [];
  const ths = head ? [...head.cells] : [];
  const keyed = ths
    .map((th, ci) => ({ th, ci, key: columnKey(th) }))
    .filter((x) => x.key);
  const apply = () => {
    for (const { th, key } of keyed) {
      const i = sort.findIndex((s) => s.key === key);
      if (i < 0) th.setAttribute("aria-sort", "none");
      else
        th.setAttribute(
          "aria-sort",
          sort[i].dir > 0 ? "ascending" : "descending",
        );
      th.dataset.mark = sortMark(sort, key);
    }
    const body = table.tBodies[0];
    if (!body) return;
    const rows = [...body.rows];
    rows.forEach((r, i) => {
      if (r.dataset.order == null) r.dataset.order = String(i);
    });
    const col = new Map(keyed.map((x) => [x.key, x.ci]));
    const sorted = applySort(
      [...rows].sort(
        (a, b) => Number(a.dataset.order) - Number(b.dataset.order),
      ),
      sort,
      (r, key) => {
        const c = r.cells[col.get(key) ?? -1];
        return sortValue(c?.dataset.sort ?? c?.textContent?.trim() ?? "");
      },
    );
    body.append(...sorted);
  };
  for (const { th, key } of keyed) {
    th.classList.add("nx-sortable");
    th.tabIndex = 0;
    th.title =
      "Sort by this column: click again to reverse, a third time for the original order · Shift+click adds a further sort";
    drive(th, { id: opts.drive.id, row: key });
    /** @param {boolean} add */
    const go = (add) => {
      sort = nextSort(sort, key, add);
      if (opts.remember) keepSort(opts.remember, sort);
      apply();
    };
    th.addEventListener("click", (e) => go(e.shiftKey));
    th.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        go(e.shiftKey);
      }
    });
  }
  apply();
  return { apply, sort: () => [...sort] };
}

/**
 * An active-filter chip: what is on, × turns it off (the Live view
 * control `drive`).
 * @param {string} label
 * @param {() => void} onClear
 * @param {Drive} d
 */
export function filterChip(label, onClear, d) {
  const x = h(
    "button",
    {
      type: "button",
      "aria-label": `Clear ${label}`,
      title: "Turn this filter off",
    },
    "×",
  );
  x.addEventListener("click", onClear);
  drive(x, d);
  return h("span", { class: "nx-filterchip" }, label, x);
}

/**
 * Where a floating card goes: centred under its element, above it when
 * there is no room below, never past the screen's edges (8 px margin).
 * @param {{left: number, top: number, bottom: number, width: number}} r
 *   the element's box
 * @param {number} w the card's width
 * @param {number} ht the card's height
 * @param {{w: number, h: number}} view the viewport
 * @returns {{left: number, top: number}}
 */
export function tipPlace(r, w, ht, view) {
  const left = Math.min(
    Math.max(8, r.left + r.width / 2 - w / 2),
    view.w - w - 8,
  );
  let top = r.bottom + 8;
  if (top + ht > view.h - 8) top = r.top - ht - 8;
  return { left: Math.round(left), top: Math.round(top) };
}

/**
 * One floating detail card for hover AND keyboard focus (DESIGN_LANGUAGE
 * §10). It sits under (or above) its element, never under the pointer, so
 * it never blocks the next hover. One per page; `stop()` removes it.
 * `cls` adds a page's own class to it (it lives on <body>).
 * @param {{cls?: string}} [opts]
 */
export function hoverCard(opts = {}) {
  const tip = h("div", {
    class: `nx-tip${opts.cls ? ` ${opts.cls}` : ""}`,
    role: "tooltip",
  });
  tip.hidden = true;
  document.body.append(tip);
  const hide = () => {
    tip.hidden = true;
  };
  /**
   * @param {HTMLElement} target
   * @param {() => import("./dom.js").Child[] | null} fill
   */
  const attach = (target, fill) => {
    const show = () => {
      const c = fill();
      if (!c || !target.isConnected) return;
      tip.replaceChildren(...h("div", null, c).childNodes);
      tip.hidden = false;
      const p = tipPlace(
        target.getBoundingClientRect(),
        tip.offsetWidth,
        tip.offsetHeight,
        { w: innerWidth, h: innerHeight },
      );
      tip.style.left = `${p.left}px`;
      tip.style.top = `${p.top}px`;
    };
    target.addEventListener("pointerenter", show);
    target.addEventListener("pointerleave", hide);
    target.addEventListener("focus", show);
    target.addEventListener("blur", hide);
  };
  return { el: tip, attach, hide, stop: () => tip.remove() };
}

/** @type {ReturnType<typeof hoverCard> | null} */
let pageCard = null;

/**
 * The one hover card a page shares (made on first use, made again after a
 * page swap removed it): `pageTip().attach(target, fill)`, `.hide()`.
 */
export function pageTip() {
  if (!pageCard?.el.isConnected) pageCard = hoverCard();
  return pageCard;
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
  /** @type {(HTMLElement & {dismiss: () => void}) | null} */
  let t = null;
  t = kpToast(text, {
    region,
    ms: opts.ms ?? TOAST_MS,
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
  }
  const made = t;
  return () => made.dismiss();
}

/**
 * Where a row menu opens: under its button's right edge, or above the
 * button near the foot of the screen so every item shows.
 * @param {{top: number, bottom: number, right: number}} r the button's box
 * @param {number} ht the menu's height
 * @param {{w: number, h: number, x: number, y: number}} view viewport size
 *   and scroll
 * @param {number} width the menu's width
 * @returns {{top: number, left: number}} document coordinates
 */
export function menuPlace(r, ht, view, width) {
  const left = Math.max(8, r.right + view.x - width);
  const top =
    r.bottom + ht + 8 > view.h
      ? Math.max(8, r.top + view.y - ht - 4)
      : r.bottom + view.y + 4;
  return { top, left };
}

/**
 * The keys of an open `role=menu`: ↓ / ↑ move the focus to the next or
 * previous item (wrapping), Home / End to the first or last.
 * @param {HTMLElement} menu
 * @param {KeyboardEvent} e
 * @returns {boolean} whether the key was one of them
 */
export function menuKey(menu, e) {
  const items = /** @type {HTMLElement[]} */ ([
    ...menu.querySelectorAll('[role="menuitem"]'),
  ]);
  if (!items.length) return false;
  const i = items.indexOf(/** @type {HTMLElement} */ (document.activeElement));
  const to =
    e.key === "ArrowDown"
      ? (i + 1) % items.length
      : e.key === "ArrowUp"
        ? (i - 1 + items.length) % items.length
        : e.key === "Home"
          ? 0
          : e.key === "End"
            ? items.length - 1
            : null;
  if (to == null) return false;
  e.preventDefault();
  items[to].focus({ preventScroll: true });
  return true;
}

/** @type {{close: () => void, anchor: HTMLElement} | null} */
let openRowMenu = null;

/**
 * A row's menu (Edit…, Run now, Delete) as a NON-modal `<dialog>`, so the
 * page stays usable and Live view presses its items by their dialog
 * control `name` (drivable.js) while it is open. One open at a time; a
 * second click on its button, Escape, a click outside or picking an item
 * closes it, and Escape gives the focus back to its button; ↑ ↓ Home End
 * move between the items. The button says whether it is open
 * (`aria-expanded`).
 * @param {{anchor: HTMLElement, label: string, title: string,
 *   items: {name: string, label: string, hint: string, run: () => void,
 *   danger?: boolean}[], width?: number}} spec
 * @returns {{el: HTMLDialogElement, close: () => void} | null} null when
 *   the click closed the menu its button had open
 */
export function rowMenu(spec) {
  if (openRowMenu?.anchor === spec.anchor) {
    openRowMenu.close();
    return null;
  }
  openRowMenu?.close();
  const width = spec.width ?? 260;
  const r = spec.anchor.getBoundingClientRect();
  const m = /** @type {HTMLDialogElement} */ (
    h(
      "dialog",
      { class: "nx-rowmenu", role: "menu", "aria-label": spec.label },
      h("h2", { class: "kp-dialog__title nx-vh" }, spec.title),
      spec.items.map((it) =>
        dialogControl(
          h(
            "button",
            {
              type: "button",
              role: "menuitem",
              class: it.danger ? "danger" : null,
              onclick: () => {
                close();
                it.run();
              },
            },
            h("b", null, it.label),
            h("span", null, it.hint),
          ),
          it.name,
        ),
      ),
    )
  );
  m.style.width = `${width}px`;
  const close = () => {
    if (m.open) m.close();
    m.remove();
  };
  const outside = (/** @type {PointerEvent} */ e) => {
    const t = /** @type {Node} */ (e.target);
    if (!m.contains(t) && !spec.anchor.contains(t)) close();
  };
  m.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      spec.anchor.focus();
    } else menuKey(m, e);
  });
  const handle = { el: m, close, anchor: spec.anchor };
  m.addEventListener("close", () => {
    document.removeEventListener("pointerdown", outside);
    spec.anchor.setAttribute("aria-expanded", "false");
    m.remove();
    if (openRowMenu === handle) openRowMenu = null;
  });
  document.addEventListener("pointerdown", outside);
  spec.anchor.setAttribute("aria-haspopup", "menu");
  spec.anchor.setAttribute("aria-expanded", "true");
  // show() focuses the first item, which scrolls the page to wherever the
  // dialog sits at that moment; keep the page where the user clicked and
  // place the menu against the button from there, by its real width.
  const view = { w: innerWidth, h: innerHeight, x: scrollX, y: scrollY };
  m.style.top = `${r.bottom + view.y + 4}px`;
  m.style.left = `${Math.max(8, r.right + view.x - width)}px`;
  document.body.append(m);
  m.show();
  if (scrollX !== view.x || scrollY !== view.y) scrollTo(view.x, view.y);
  const p = menuPlace(r, m.offsetHeight, view, m.offsetWidth);
  m.style.top = `${p.top}px`;
  m.style.left = `${p.left}px`;
  openRowMenu = handle;
  /** @type {HTMLElement | null} */ (m.querySelector("button"))?.focus({
    preventScroll: true,
  });
  return { el: m, close };
}

/**
 * The header's overflow `···` menu: a button (the Live view control
 * `drive`) and a small list of links. A second click on the button,
 * Escape (the focus back on the button), a click outside or picking an
 * item closes it; ↑ ↓ Home End move between the items. It listens on the
 * document only while it is open.
 * @param {{label: string, items: {label: string, hint: string,
 *   href: string, download?: string}[], drive: Drive}} spec
 * @returns {{el: HTMLElement, stop: () => void}}
 */
export function moreMenu(spec) {
  const list = h(
    "div",
    { class: "nx-menu", role: "menu", hidden: true },
    spec.items.map((it) =>
      h(
        "a",
        {
          role: "menuitem",
          href: it.href,
          download: it.download ?? null,
          onclick: () => close(),
        },
        h("b", null, it.label),
        h("span", null, it.hint),
      ),
    ),
  );
  const btn = h(
    "button",
    {
      type: "button",
      class: "nx-icon-btn",
      "aria-label": spec.label,
      "aria-haspopup": "menu",
      "aria-expanded": "false",
      title: spec.label,
      onclick: () => (list.hidden ? open() : close()),
    },
    "···",
  );
  drive(btn, spec.drive);
  const wrap = h("div", { class: "nx-more" }, btn, list);
  /** @param {MouseEvent} e */
  const outside = (e) => {
    if (!wrap.contains(/** @type {Node} */ (e.target))) close();
  };
  /** @param {KeyboardEvent} e */
  const keys = (e) => {
    if (list.hidden) return;
    if (e.key === "Escape") {
      e.preventDefault();
      close();
      btn.focus();
    } else if (
      list.contains(/** @type {Node} */ (e.target)) ||
      e.target === btn
    )
      menuKey(list, e);
  };
  const open = () => {
    list.hidden = false;
    btn.setAttribute("aria-expanded", "true");
    document.addEventListener("click", outside);
    document.addEventListener("keydown", keys);
  };
  const close = () => {
    list.hidden = true;
    btn.setAttribute("aria-expanded", "false");
    document.removeEventListener("click", outside);
    document.removeEventListener("keydown", keys);
  };
  return { el: wrap, stop: close };
}

/**
 * A keyboard key, as printed beside the action it triggers.
 * @param {string} k
 */
export const kbd = (k) => h("kbd", { class: "nx-kbd" }, k);

/**
 * The page's compact line of shortcuts ("/ search containers · …"). A key
 * may be several keys for one action (`[["↑", "↓"], "move"]`).
 * @param {[string | string[], string][]} pairs keys, what they do
 */
export const keyRow = (pairs) =>
  h(
    "p",
    { class: "nx-keys", "aria-label": "Keyboard shortcuts" },
    ...pairs.map(([k, what]) =>
      h("span", null, ...(Array.isArray(k) ? k : [k]).map(kbd), what),
    ),
  );

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

/**
 * Loads a page's own stylesheet (`/css/pages/<page>.css`) once, when the
 * page is first drawn, instead of every page's sheet on every page.
 * @param {string} href
 * @returns {HTMLLinkElement}
 */
export function ensureStyle(href) {
  const had = /** @type {HTMLLinkElement | null} */ (
    document.querySelector(`link[data-page-style="${CSS.escape(href)}"]`)
  );
  if (had) return had;
  const l = /** @type {HTMLLinkElement} */ (
    h("link", { rel: "stylesheet", href, "data-page-style": href })
  );
  document.head.append(l);
  return l;
}

/** @typedef {"ok" | "warn" | "bad" | "info" | "live" | ""} Tone */

/**
 * A small status chip: a count, a state word ("3", "unreadable"), in a
 * tone. With `dot`, a status dot leads the word.
 * @param {import("./dom.js").Child} text
 * @param {{tone?: Tone, dot?: boolean, title?: string, label?: string}} [o]
 *   `label`: what a screen reader says instead of the text ("!")
 */
export function chip(text, o = {}) {
  return h(
    "span",
    {
      class: `nx-chip${o.tone ? ` nx-chip--${o.tone}` : ""}${o.dot ? " nx-chip--dot" : ""}`,
      title: o.title ?? null,
      "aria-label": o.label ?? null,
    },
    text,
  );
}

/**
 * A status dot, with its word beside it (the word is what a screen reader
 * reads; a dot without one is decorative).
 * @param {Tone} tone
 * @param {string} [word]
 */
export function dot(tone, word) {
  return h(
    "span",
    {
      class: `nx-dot${tone ? ` nx-dot--${tone}` : ""}`,
      "aria-hidden": word ? null : "true",
    },
    word ?? "",
  );
}

/**
 * A meter on its own (a KPI tile's, a table cell's): a bar `pct` full,
 * with a tick at `mark` and the warn / bad tone (`meterParts`).
 * @param {Meter} m
 * @returns {{el: HTMLElement, set: (m: Meter) => void}}
 */
export function meter(m) {
  const e = h("span", { class: "nx-meter", "aria-hidden": "true" });
  const set = (/** @type {Meter} */ next) => {
    const p = meterParts(next);
    e.className = `nx-meter${p.tone ? ` nx-meter--${p.tone}` : ""}`;
    const fill = h("i");
    fill.style.width = `${p.fill}%`;
    /** @type {HTMLElement[]} */
    const parts = [fill];
    if (p.mark != null) {
      const tick = h("b", next.markTitle ? { title: next.markTitle } : null);
      tick.style.left = `${p.mark}%`;
      parts.push(tick);
    }
    e.replaceChildren(...parts);
  };
  set(m);
  return { el: e, set };
}

/**
 * One row's share of a whole as a bar with its number beside it ("41 %",
 * "2.1 GB"): the bar in `colour` (a stack's) or the first chart colour.
 * @param {number | null} pct
 * @param {string} text the number the bar stands for, always shown
 * @param {string} [colour]
 */
export function shareBar(pct, text, colour) {
  const fill = h("i");
  fill.style.width = `${meterParts({ pct }).fill}%`;
  const e = h(
    "span",
    { class: "nx-share" },
    h("span", { class: "nx-share__bar", "aria-hidden": "true" }, fill),
    h("span", { class: "nx-share__num" }, text),
  );
  if (colour) e.style.setProperty("--c", colour);
  return e;
}

/**
 * The error state of a block (DESIGN_LANGUAGE §7: loading, empty, error
 * and filled take the same footprint): what could not be read, why, the
 * fix, and Try again (the Live view control `retry.drive`).
 * @param {{what: string, why: string, fix?: string | null}} err
 * @param {{run: () => void, drive: Drive, label?: string} | null} [retry]
 */
export function failBox(err, retry = null) {
  return h(
    "div",
    {
      class: "kp-alert kp-alert--destructive nx-fail",
      role: "alert",
      "data-kp-state": "error",
    },
    h(
      "div",
      { class: "kp-alert__body" },
      h("strong", null, err.what),
      h("p", null, err.why),
      err.fix ? h("p", null, `Fix: ${err.fix}`) : null,
      retry
        ? drive(
            h(
              "button",
              {
                type: "button",
                class: "kp-button kp-button--sm",
                title: "Read it again",
                onclick: retry.run,
              },
              retry.label ?? "Try again",
            ),
            retry.drive,
          )
        : null,
    ),
  );
}

/**
 * Keyboard navigation over a list's rows (DESIGN_LANGUAGE §10): j / ↓ and
 * k / ↑ move the focus to the next or previous visible row matching
 * `selector` inside `root`, Home / End to the first or last, Enter opens
 * it; never while typing in a field or with a modifier. Returns the
 * function that stops listening.
 * @param {HTMLElement} root
 * @param {string} selector the rows (focusable: give them tabindex)
 * @param {(row: HTMLElement) => void} [onEnter]
 * @returns {() => void}
 */
export function rowKeys(root, selector, onEnter) {
  /** @param {KeyboardEvent} e */
  const key = (e) => {
    const t = /** @type {HTMLElement | null} */ (e.target);
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (t?.closest("input, textarea, select, [contenteditable]")) return;
    // redesign-config-12: a button or a link that is not itself a row
    // keeps j / k / Enter to itself.
    if (t && typingTarget(t.tagName) && !t.matches(selector)) return;
    const rows = /** @type {HTMLElement[]} */ ([
      ...root.querySelectorAll(selector),
    ]).filter((r) => !r.hidden && r.offsetParent !== null);
    if (!rows.length) return;
    const at = rows.findIndex((r) => r === t || r.contains(t));
    const to =
      e.key === "j" || e.key === "ArrowDown"
        ? Math.min(rows.length - 1, at + 1)
        : e.key === "k" || e.key === "ArrowUp"
          ? Math.max(0, at < 0 ? 0 : at - 1)
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? rows.length - 1
              : null;
    if (to != null) {
      e.preventDefault();
      rows[to].focus();
    } else if (e.key === "Enter" && at >= 0 && rows[at] === t && onEnter) {
      e.preventDefault();
      onEnter(rows[at]);
    }
  };
  root.addEventListener("keydown", key);
  return () => root.removeEventListener("keydown", key);
}

/**
 * A side panel (DESIGN_LANGUAGE §6): a modal `<dialog>` against the right
 * edge (the whole width on a phone) with a head (title, one sentence,
 * Close), a body and a foot of actions. Escape, Close and a click on the
 * backdrop close it; its buttons are found by Live view among the open
 * dialog's own (Close is the dialog control `close`).
 * @param {{title: string, desc: string, body?: import("./dom.js").Child,
 *   foot?: import("./dom.js").Child, onClose?: () => void,
 *   cls?: string}} spec
 * @returns {{el: HTMLDialogElement, body: HTMLElement, foot: HTMLElement,
 *   open: () => void, close: () => void}}
 */
export function drawer(spec) {
  const id = `nx-drawer-${++drawers}`;
  const body = h("div", { class: "nx-drawer__body" }, spec.body ?? null);
  const foot = h("div", { class: "nx-drawer__foot" }, spec.foot ?? null);
  foot.hidden = spec.foot == null;
  const close = () => {
    if (d.open) d.close();
  };
  const x = dialogControl(
    h(
      "button",
      {
        type: "button",
        class: "nx-icon-btn",
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
        class: `kp-dialog nx-drawer${spec.cls ? ` ${spec.cls}` : ""}`,
        "aria-labelledby": `${id}-h`,
      },
      h(
        "div",
        { class: "nx-drawer__head" },
        h("h2", { id: `${id}-h`, class: "kp-dialog__title" }, spec.title),
        x,
        h("p", { class: "section-head__desc" }, spec.desc),
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
 * Shift-click for a second key, the sort remembered under `remember`; the
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
      "data-kp-sort-multi": "",
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
 * A block's muted line while the page's one failure band says why.
 * @param {string} text
 */
export const failNote = (text) => h("p", { class: "nx-failnote" }, text);

/**
 * One alert for every read a page lost at once (redesign-config-11): the
 * cause once, under the header, with ONE Try again (the Live view control
 * `drive`); the blocks only say they are empty (`failNote`). Hidden while
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
      box.replaceChildren(
        h(
          "div",
          { class: "kp-alert kp-alert--destructive nx-fail", role: "alert" },
          h(
            "div",
            { class: "kp-alert__body" },
            h("strong", null, f.title),
            h("p", null, `Why: ${f.why}`),
            f.fix ? h("p", null, `What to do: ${f.fix}`) : null,
            drive(
              h(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--sm",
                  title: "Read it again",
                  onclick: retry,
                },
                "Try again",
              ),
              spec.drive,
            ),
          ),
        ),
      );
    },
  };
}
