// redesign-config (3.71.0): small LOCAL building blocks the three
// Configure-side pages share — Firewall, Settings (with Sign-in) and
// Presets — drawn as Kenny's approved demos draw them
// (~/.local/share/homelab/redesign-3.71/{firewall,settings,presets}.html
// and their cfg-aa93.js interaction kit). Shaped like the foundation's
// ui.js API where one exists, so the merge can fold them in:
//
//   pageHeader   ui.js's, plus a `meta` slot beside the title (the demo's
//                "working copy bb5dce9" / "read 3 s ago" chip)
//   tipOn        one floating detail card for hover AND keyboard focus
//                (cfg-aa93.js `tipOn`); never under the pointer
//   sortHead     a sortable header: click asc → desc → none, Shift+click
//                adds a second key; remembered per table (localStorage)
//   seg          a segmented single choice (the demo's `.nx-seg`)
//   highlight    the search's match marked inside a text
//   rowKeys      j / k move between rows, Enter opens
//   skel         one skeleton bar in the final geometry
//   dot          status = dot + word (DESIGN_LANGUAGE §4)
//
// Styled by css/pages/configkit.css under the `cf-` prefix.

import { el, ensureStyle } from "./hostkit.js";

export { el, ensureStyle };

/** @typedef {import("./hostkit.js").Child} Child */

/** Load the shared sheet plus the page's own. @param {string} page */
export function pageStyles(page) {
  ensureStyle("/css/pages/configkit.css");
  ensureStyle(`/css/pages/${page}.css`);
}

/**
 * The page header as the demos draw it: the title with its meta chips on
 * one line, the one-sentence description right under it, the actions
 * against the right edge across both (secondary first, the ONE primary
 * last). The description is the title's next sibling paragraph
 * (invariants 32, 36, 40, 41).
 * @param {{title: string, desc: string, meta?: Child[], actions?: Child[],
 *   primary?: Node | null}} spec
 */
export function pageHeader(spec) {
  const title = /** @type {HTMLHeadingElement} */ (el("h1", null, spec.title));
  const desc = /** @type {HTMLParagraphElement} */ (
    el("p", { class: "cf-head__desc section-head__desc" }, spec.desc)
  );
  const meta = el("div", { class: "cf-head__meta" }, spec.meta ?? []);
  const actions = el(
    "div",
    { class: "cf-head__actions actions-row" },
    spec.actions ?? [],
    spec.primary ?? null,
  );
  const e = el("header", { class: "cf-head" }, title, desc, meta, actions);
  return { el: e, title, desc, meta, actions };
}

/** A small chip. @param {Child} text @param {string} [tone] */
export const chip = (text, tone) =>
  el("span", { class: `cf-chip${tone ? ` cf-chip--${tone}` : ""}` }, text);

/** Status = dot + word. @param {string} tone @param {Child} word */
export const dot = (tone, word) =>
  el("span", { class: `cf-dot cf-dot--${tone}` }, word);

/** A skeleton bar of width `w`. @param {string} [w] */
export function skel(w = "70%") {
  const s = el("span", { class: "cf-sk", "aria-hidden": "true" }, "·");
  s.style.setProperty("--w", w);
  return s;
}

/** The demo's `<kbd>` hint. @param {string} k */
export const kbd = (k) => el("kbd", { class: "nx-kbd" }, k);

// ── tip ──────────────────────────────────────────────────────────────────

/** @type {HTMLElement | null} */
let tipEl = null;
const tip = () => {
  if (!tipEl || !tipEl.isConnected) {
    tipEl = el("div", { class: "cf-tip", role: "tooltip", hidden: true });
    document.body.append(tipEl);
  }
  return tipEl;
};

/** Hide the floating card (page cleanup, Esc). */
export const hideTip = () => {
  if (tipEl) tipEl.hidden = true;
};

/**
 * One floating detail card for hover AND keyboard focus; `fill` returns
 * its content (or null for none). It sits under (or above) the element,
 * never under the pointer.
 * @param {HTMLElement} target
 * @param {() => Child[] | null} fill
 */
export function tipOn(target, fill) {
  const show = () => {
    const c = fill();
    if (!c) return;
    const t = tip();
    t.replaceChildren(...el("div", null, c).childNodes);
    t.hidden = false;
    const r = target.getBoundingClientRect();
    const tw = t.offsetWidth;
    const th = t.offsetHeight;
    const left = Math.min(
      Math.max(8, r.left + r.width / 2 - tw / 2),
      innerWidth - tw - 8,
    );
    let top = r.bottom + 8;
    if (top + th > innerHeight - 8) top = r.top - th - 8;
    t.style.left = `${left + scrollX}px`;
    t.style.top = `${top + scrollY}px`;
  };
  target.addEventListener("pointerenter", show);
  target.addEventListener("pointerleave", hideTip);
  target.addEventListener("focus", show);
  target.addEventListener("blur", hideTip);
}

// ── sorting ──────────────────────────────────────────────────────────────

/** @typedef {{key: string, dir: 1 | -1}[]} SortState */

/**
 * The sort a table had last time on this browser (DESIGN_LANGUAGE §6:
 * remembered per table).
 * @param {string} name
 * @returns {SortState}
 */
export function rememberedSort(name) {
  try {
    const v = JSON.parse(localStorage.getItem(`cf-sort:${name}`) ?? "[]");
    return Array.isArray(v)
      ? v.filter(
          (x) => typeof x?.key === "string" && (x.dir === 1 || x.dir === -1),
        )
      : [];
  } catch {
    return [];
  }
}

/** @param {string} name @param {SortState} state */
function keepSort(name, state) {
  try {
    localStorage.setItem(`cf-sort:${name}`, JSON.stringify(state));
  } catch {
    /* private window: the sort lives for this visit only */
  }
}

/**
 * The next sort after a click on `key` (pure): a plain click sorts by it
 * alone, ascending → descending → none; Shift+click adds it as a further
 * key, or flips it when it is one already.
 * @param {SortState} state
 * @param {string} key
 * @param {boolean} shift
 * @returns {SortState}
 */
export function nextSort(state, key, shift) {
  const i = state.findIndex((s) => s.key === key);
  if (shift) {
    if (i < 0) return [...state, { key, dir: 1 }];
    return state.map((s, j) =>
      j === i ? { key, dir: /** @type {1 | -1} */ (-s.dir) } : s,
    );
  }
  if (i === 0 && state.length === 1)
    return state[0].dir === 1 ? [{ key, dir: -1 }] : [];
  return [{ key, dir: 1 }];
}

/**
 * Rows in the sort's order (pure; a stable sort, numbers as numbers).
 * @template T
 * @param {T[]} rows
 * @param {SortState} state
 * @param {(row: T, key: string) => string | number} get
 * @returns {T[]}
 */
export function applySort(rows, state, get) {
  return [...rows].sort((a, b) => {
    for (const { key, dir } of state) {
      const x = get(a, key);
      const y = get(b, key);
      if (x < y) return -dir;
      if (x > y) return dir;
    }
    return 0;
  });
}

/**
 * A sortable header cell: `state` is changed in place and remembered
 * under `name`, then `onChange` repaints.
 * @param {{label: string, key: string, name: string, state: SortState,
 *   onChange: () => void, cls?: string}} spec
 */
export function sortHead(spec) {
  const i = spec.state.findIndex((s) => s.key === spec.key);
  const s = spec.state[i];
  const mark = s
    ? `${s.dir > 0 ? "↑" : "↓"}${spec.state.length > 1 ? i + 1 : ""}`
    : "↕";
  return el(
    "th",
    {
      class: spec.cls ?? null,
      scope: "col",
      "aria-sort": s ? (s.dir > 0 ? "ascending" : "descending") : "none",
    },
    el(
      "button",
      {
        type: "button",
        class: "cf-sort",
        title:
          "Sort: click for ascending, again for descending, a third time for none · Shift+click adds a second sort",
        onclick: (/** @type {MouseEvent} */ e) => {
          const next = nextSort(spec.state, spec.key, e.shiftKey);
          spec.state.splice(0, spec.state.length, ...next);
          keepSort(spec.name, spec.state);
          spec.onChange();
        },
      },
      spec.label,
      el("span", { class: "cf-sort__mark", "aria-hidden": "true" }, mark),
    ),
  );
}

// ── small controls ───────────────────────────────────────────────────────

/**
 * A segmented single choice (the demo's `.nx-seg`): one value pressed.
 * @param {{label: string, options: {value: string, label: string,
 *   hint?: string}[], value: string, onChange: (v: string) => void,
 *   mark?: (b: HTMLElement, value: string) => void}} spec
 */
export function seg(spec) {
  let value = spec.value;
  const box = el("div", {
    class: "cf-seg",
    role: "group",
    "aria-label": spec.label,
  });
  const paint = () => {
    box.replaceChildren(
      ...spec.options.map((o) => {
        const b = el(
          "button",
          {
            type: "button",
            "aria-pressed": String(o.value === value),
            title: o.hint ?? `Show ${o.label}`,
            "data-value": o.value,
            onclick: () => {
              if (value === o.value) return;
              value = o.value;
              paint();
              spec.onChange(value);
            },
          },
          o.label,
        );
        spec.mark?.(b, o.value);
        return b;
      }),
    );
  };
  paint();
  return {
    el: box,
    get: () => value,
    set: (/** @type {string} */ v) => {
      value = v;
      paint();
    },
    /** @param {{value: string, label: string, hint?: string}[]} opts */
    setOptions: (opts) => {
      spec.options = opts;
      paint();
    },
  };
}

/**
 * The text with the query's first match marked (case-insensitive).
 * @param {string | null | undefined} text
 * @param {string} q lower-case
 * @returns {Child}
 */
export function highlight(text, q) {
  if (!text) return text ?? "";
  if (!q) return text;
  const i = text.toLowerCase().indexOf(q);
  if (i < 0) return text;
  return [
    text.slice(0, i),
    el("mark", { class: "cf-mark" }, text.slice(i, i + q.length)),
    text.slice(i + q.length),
  ];
}

/**
 * j / k (and the arrows) move between `selector` items inside `root`;
 * Enter activates the focused one.
 * @param {HTMLElement} root
 * @param {string} selector
 * @param {(item: HTMLElement) => void} onEnter
 */
export function rowKeys(root, selector, onEnter) {
  root.addEventListener("keydown", (e) => {
    if (!["j", "k", "ArrowDown", "ArrowUp", "Enter"].includes(e.key)) return;
    const t = /** @type {HTMLElement} */ (e.target);
    if (/INPUT|TEXTAREA|SELECT|BUTTON|A/.test(t.tagName)) return;
    const items = /** @type {HTMLElement[]} */ ([
      ...root.querySelectorAll(selector),
    ]).filter((x) => x.offsetParent);
    const i = items.indexOf(t);
    if (e.key === "Enter") {
      if (i >= 0) {
        e.preventDefault();
        onEnter(items[i]);
      }
      return;
    }
    e.preventDefault();
    const step = e.key === "j" || e.key === "ArrowDown" ? 1 : -1;
    items[Math.max(0, Math.min(items.length - 1, i + step))]?.focus();
  });
}

/**
 * A card's error (DESIGN_LANGUAGE §7): what failed, why (the host's
 * reason), what to do, and Try again, inside the card it belongs to.
 * @param {import("../doctor.js").RouteError} e
 * @param {() => void} retry
 */
export function failBox(e, retry) {
  return el(
    "div",
    { class: "kp-alert kp-alert--destructive cf-fail", role: "alert" },
    el(
      "div",
      { class: "kp-alert__body" },
      el("strong", null, `Could not read ${e.what}`),
      el("p", null, e.why),
      e.fix ? el("p", null, e.fix) : null,
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          title: "Read it again",
          onclick: retry,
        },
        "Try again",
      ),
    ),
  );
}
