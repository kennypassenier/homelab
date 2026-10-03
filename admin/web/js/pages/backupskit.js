// redesign-backups (Backups redesign 3.71): the Backups page's own copies of the
// shared pieces the redesign's foundation (branch redesign-371-foundation,
// `admin/web/js/ui.js`) builds for every page — page header, live status,
// KPI strip, attention band, card section, hover card, sort header, filter
// chip, stack identity mark. Same names and the same call shapes as the
// foundation's, so merging onto the new shell is an import swap: replace
// these with `../ui.js`'s and delete this file and the `.bk-head`,
// `.bk-kpi*`, `.bk-attention*`, `.bk-card*`, `.bk-tip`, `.bk-chip`,
// `.bk-mark` rules in app.css. Only what the foundation's versions do not
// cover is marked below (a KPI tile that toggles a filter; a card foot).

import { h } from "../dom.js";
import { agoText } from "../format.js";

/**
 * @template {HTMLElement} E
 * @param {E} el
 * @param {string} ev
 * @param {(e: any) => void} fn
 * @returns {E}
 */
export function on(el, ev, fn) {
  el.addEventListener(ev, fn);
  return el;
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
 * A pulsing placeholder in the final geometry (DESIGN_LANGUAGE §7).
 * @param {string} [width]
 */
export function skeleton(width = "70%") {
  return h("span", {
    class: "kp-skeleton bk-sk",
    style: `inline-size: ${width}`,
    "aria-hidden": "true",
  });
}

/**
 * The live freshness of the page: a dot and "read 7 s ago", ticking.
 * @param {string} [verb]
 * @returns {{el: HTMLElement, set: (at: number | null, words?: string) => void}}
 */
export function liveStatus(verb = "read") {
  const el = h("span", { class: "bk-live", role: "status" });
  /** @type {number | null} */
  let at = null;
  /** @type {string | null} */
  let override = null;
  const paint = () => {
    el.textContent =
      override ?? agoText(verb, at, Math.floor(Date.now() / 1000));
    el.dataset.state = override ? "busy" : at == null ? "none" : "ok";
  };
  const timer = setInterval(() => {
    if (!el.isConnected) clearInterval(timer);
    else paint();
  }, 1000);
  paint();
  return {
    el,
    set: (t, words) => {
      at = t;
      override = words ?? null;
      paint();
    },
  };
}

/**
 * The page header (DESIGN_LANGUAGE §1.1): one title row (title left; live
 * status and the actions against the right edge, the primary one last),
 * the one-sentence description right under it.
 * @param {{title: string, desc: string, live?: string, actions?: Node[],
 *   primary?: Node | null}} spec
 */
export function pageHeader(spec) {
  const title = h("h1", null, spec.title);
  const live = spec.live ? liveStatus(spec.live) : null;
  const actions = h(
    "div",
    { class: "actions-row bk-head__actions" },
    ...(spec.actions ?? []),
    ...(spec.primary ? [spec.primary] : []),
  );
  const right = h(
    "div",
    { class: "bk-head__right" },
    ...(live ? [live.el] : []),
    actions,
  );
  const desc = h("p", { class: "bk-head__desc" }, spec.desc);
  const el = h(
    "header",
    { class: "bk-head" },
    h("div", { class: "title-row" }, title, right),
    desc,
  );
  return { el, title, desc, actions, live };
}

/**
 * @typedef {{key: string, label: string, value?: string, unit?: string,
 *   ctx?: string, ctxTone?: "ok" | "warn" | "bad" | null,
 *   tone?: "warn" | "bad" | null, title?: string,
 *   toggle?: {pressed: boolean, onToggle: () => void,
 *     drive?: (el: HTMLElement) => void}}} Kpi
 *   `toggle` (not in the foundation's `kpi`, which links to its detail
 *   instead): the tile is a button that turns a filter on the page on or
 *   off, `aria-pressed` saying which.
 */

/**
 * One KPI tile (DESIGN_LANGUAGE §1.2); `set` repaints it in place, so the
 * strip never reflows (rule 6).
 * @param {Kpi} k
 */
export function kpi(k) {
  const label = h("span", { class: "bk-kpi__label" });
  const hint = h("span", { class: "bk-kpi__hint", "aria-hidden": "true" });
  const value = h("span", { class: "bk-kpi__value" });
  const ctx = h("span", { class: "bk-kpi__ctx" });
  const el = k.toggle
    ? h("button", { type: "button", class: "bk-kpi bk-kpi--toggle" })
    : h("div", { class: "bk-kpi" });
  el.append(label, hint, value, ctx);
  if (k.toggle) {
    el.addEventListener("click", () => cur.toggle?.onToggle());
    k.toggle.drive?.(el);
  }
  /** @type {Kpi} */
  let cur = { ...k };
  /** @type {boolean} */
  let loading = false;
  const set = (/** @type {Partial<Kpi> & {loading?: boolean}} */ next) => {
    if (next.loading != null) loading = next.loading;
    cur = { ...cur, ...next };
    label.textContent = cur.label;
    value.replaceChildren(
      ...(loading
        ? [skeleton("3ch")]
        : [
            cur.value ?? "—",
            ...(cur.unit ? [h("small", null, cur.unit)] : []),
          ]),
    );
    ctx.replaceChildren(
      loading
        ? skeleton("80%")
        : h(
            "span",
            { class: cur.ctxTone ? `bk-dot bk-dot--${cur.ctxTone}` : "" },
            cur.ctx ?? "",
          ),
    );
    el.dataset.tone = cur.tone ?? "";
    if (cur.title) el.title = cur.title;
    if (cur.toggle) {
      el.setAttribute("aria-pressed", String(cur.toggle.pressed));
      hint.textContent = cur.toggle.pressed ? "filtering ×" : "filter";
    }
  };
  set({});
  return { el, set };
}

/**
 * A row of KPI tiles; while `loading` each is its own skeleton in the
 * final geometry.
 * @param {Kpi[]} tiles
 * @param {{loading?: boolean}} [opts]
 */
export function kpiStrip(tiles, opts = {}) {
  /** @type {Map<string, ReturnType<typeof kpi>>} */
  const map = new Map();
  const el = h("section", {
    class: "bk-kpis",
    "aria-label": "Backup totals",
  });
  for (const t of tiles) {
    const k = kpi(t);
    if (opts.loading) k.set({ loading: true });
    map.set(t.key, k);
    el.append(k.el);
  }
  return { el, tiles: map };
}

/**
 * @typedef {{key?: string, tone: "warn" | "bad" | "info", title: string,
 *   text?: string, action?: Node | null}} Attention
 */

/**
 * The attention band (DESIGN_LANGUAGE §1.3): one alert per problem, worst
 * first, each with its own fix; absent (zero height) when nothing needs a
 * person.
 * @param {Attention[]} [items]
 */
export function attentionBand(items = []) {
  const el = h("div", {
    class: "bk-attention",
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
            class: `kp-alert kp-alert--${a.tone === "bad" ? "destructive" : a.tone === "warn" ? "warning" : "info"} bk-attention__item`,
            role: a.tone === "bad" ? "alert" : "status",
            ...(a.key ? { "data-key": a.key } : {}),
          },
          h(
            "span",
            { class: "bk-attention__icon", "aria-hidden": "true" },
            a.tone === "info" ? "i" : "!",
          ),
          h(
            "div",
            { class: "bk-attention__text" },
            h("strong", null, a.title),
            ...(a.text ? [h("span", null, a.text)] : []),
          ),
          h(
            "div",
            { class: "bk-attention__act" },
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
 * A section of the page as a card (DESIGN_LANGUAGE §5): a head with its
 * title and one sentence (rule 8) on the left and its tools on the right,
 * the body, and (not in the foundation's `section`) an optional foot line.
 * `collapsible` makes it a `<details>`, folded unless `open`.
 * @param {{title: string, desc: string, id?: string, tools?: Node[],
 *   collapsible?: boolean, open?: boolean, badge?: Node | null,
 *   cls?: string, tag?: "section" | "aside"}} spec
 */
export function section(spec) {
  const titleEl = h("h2", { id: `${spec.id ?? "bk"}-h` }, spec.title);
  const head = h(
    "div",
    { class: "bk-card__head" },
    titleEl,
    ...(spec.badge ? [spec.badge] : []),
    h("p", { class: "bk-card__desc" }, spec.desc),
    ...(spec.tools?.length
      ? [h("div", { class: "bk-card__tools" }, ...spec.tools)]
      : []),
  );
  const body = h("div", { class: "bk-card__body" });
  const foot = h("div", { class: "bk-card__foot" });
  foot.hidden = true;
  /** @type {HTMLElement} */
  let el;
  if (spec.collapsible) {
    el = h(
      "details",
      {
        class: `kp-card bk-card bk-card--fold ${spec.cls ?? ""}`.trim(),
        ...(spec.id ? { id: spec.id } : {}),
        "aria-labelledby": titleEl.id,
      },
      h("summary", null, head),
      body,
      foot,
    );
    if (spec.open) /** @type {HTMLDetailsElement} */ (el).open = true;
  } else {
    el = h(
      spec.tag ?? "section",
      {
        class: `kp-card bk-card ${spec.cls ?? ""}`.trim(),
        ...(spec.id ? { id: spec.id } : {}),
        "aria-labelledby": titleEl.id,
      },
      head,
      body,
      foot,
    );
  }
  return { el, head, body, foot };
}

/**
 * One floating detail card for hover AND keyboard focus (DESIGN_LANGUAGE
 * §10). It sits under (or above) its element, never under the pointer, so
 * it never blocks the next hover. One per page; `stop()` removes it.
 */
export function hoverCard() {
  const tip = h("div", { class: "bk-tip", role: "tooltip" });
  tip.hidden = true;
  document.body.append(tip);
  const hide = () => {
    tip.hidden = true;
  };
  /**
   * @param {HTMLElement} el
   * @param {() => Node[] | null} fill
   */
  const attach = (el, fill) => {
    const show = () => {
      const c = fill();
      if (!c || !el.isConnected) return;
      tip.replaceChildren(...c);
      tip.hidden = false;
      const r = el.getBoundingClientRect();
      const tw = tip.offsetWidth;
      const th = tip.offsetHeight;
      const left = Math.min(
        Math.max(8, r.left + r.width / 2 - tw / 2),
        innerWidth - tw - 8,
      );
      let top = r.bottom + 8;
      if (top + th > innerHeight - 8) top = r.top - th - 8;
      tip.style.left = `${Math.round(left)}px`;
      tip.style.top = `${Math.round(top)}px`;
    };
    el.addEventListener("pointerenter", show);
    el.addEventListener("pointerleave", hide);
    el.addEventListener("focus", show);
    el.addEventListener("blur", hide);
  };
  return { el: tip, attach, hide, stop: () => tip.remove() };
}

/**
 * A sortable column header: click sorts (ascending, descending, none),
 * Shift+click adds a further sort key.
 * @param {string} label
 * @param {{key: string, dir: number}[]} sort
 * @param {string} key
 * @param {(add: boolean) => void} onSort
 * @param {{cls?: string, drive?: (el: HTMLElement) => void}} [o]
 */
export function sortHead(label, sort, key, onSort, o = {}) {
  const i = sort.findIndex((s) => s.key === key);
  const s = sort[i];
  const btn = h(
    "button",
    {
      type: "button",
      class: "bk-sort",
      title:
        "Sort: click for ascending, again for descending, a third time for none · Shift+click adds a second sort",
    },
    label,
    h(
      "span",
      { class: "bk-sort__mark", "aria-hidden": "true" },
      s ? `${s.dir > 0 ? "↑" : "↓"}${sort.length > 1 ? i + 1 : ""}` : "↕",
    ),
  );
  btn.addEventListener("click", (e) => onSort(e.shiftKey));
  o.drive?.(btn);
  return h(
    "th",
    {
      scope: "col",
      ...(o.cls ? { class: o.cls } : {}),
      "aria-sort": s ? (s.dir > 0 ? "ascending" : "descending") : "none",
    },
    btn,
  );
}

/**
 * An active-filter chip: what is on, × turns it off.
 * @param {string} label
 * @param {() => void} onClear
 * @param {(el: HTMLElement) => void} [drive]
 */
export function filterChip(label, onClear, drive) {
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
  drive?.(x);
  return h("span", { class: "bk-chip" }, label, x);
}

/**
 * A stack's identity mark: its own hue (the topology's, `stackHues`), the
 * same square wherever the stack is named.
 * @param {number} hue
 */
export function stackMark(hue) {
  return h("span", {
    class: "bk-mark",
    style: `--stack-hue: ${hue}`,
    "aria-hidden": "true",
  });
}

/**
 * A keyboard key as printed beside the action it triggers.
 * @param {string} k
 */
export const kbd = (k) => h("kbd", { class: "bk-kbd" }, k);
