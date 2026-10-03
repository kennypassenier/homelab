// redesign-stackhub (3.71.0): the stack hub's LOCAL stand-ins for what the
// shared ui.js does not have yet, named and shaped so they can move there
// (reported to the foundation): the hub header (identity mark, title,
// state, meta chips, actions with a grouped menu), a grouped menu of
// buttons, the side filter column of a log explorer (DESIGN_LANGUAGE §12),
// a card foot, a status dot, a chip and the 14-night strip. The key row
// and the one-of segmented switch are the Host page kit's (hostkit.js),
// not copies of them (review 6). Styled by `css/pages/stack.css` under `sh-`, on
// top of the foundation's `nx-` classes where those exist (nx-head,
// title-row, nx-head-right, actions-row).

import { el, ensureStyle } from "./hostkit.js";
import { liveStatus, stackMark } from "../ui.js";
import { menuSignature } from "../stackhub.js";

export { el, ensureStyle };

/** @typedef {import("./hostkit.js").Child} Child */

/**
 * A status as a dot and a word (DESIGN_LANGUAGE §4): the dot carries the
 * colour, the word the meaning.
 * @param {string} tone "ok" | "warn" | "bad" | "info" | "unknown" | ""
 * @param {Child} [text]
 */
export const dot = (tone, text) =>
  el("span", { class: `sh-dot${tone ? ` sh-dot--${tone}` : ""}` }, text ?? "");

/**
 * A small chip (the header's meta row, the feed's "who").
 * @param {{label: string, tone?: string | null, mono?: boolean,
 *   title?: string}} c
 */
export const chip = (c) =>
  el(
    "span",
    {
      class: `sh-chip${c.tone ? ` sh-chip--${c.tone}` : ""}${c.mono ? " mono" : ""}`,
      title: c.title ?? null,
    },
    c.label,
  );

/**
 * The hub's header (flows/stack-hub.html): the stack's identity mark, its
 * name and state on the title row with the live status and the actions
 * against the right edge (Back up · Update · Deploy, the primary one, then
 * More), the one-sentence description under it, and the meta chips.
 * Markup the whole-screen invariants read: `.title-row` holding the h1,
 * the description its next sibling `p`, the buttons in `.actions-row`.
 * @param {{name: string, desc: string, actions: Node[], primary: Node,
 *   more: Node}} spec
 */
export function hubHeader(spec) {
  const title = /** @type {HTMLHeadingElement} */ (el("h1", null, spec.name));
  const state = el("span", { class: "sh-head__state", id: "stack-state" });
  const live = liveStatus("updated");
  const actions = el(
    "div",
    { class: "actions-row nx-head-actions sh-head__actions" },
    spec.actions,
    spec.primary,
    spec.more,
  );
  const right = el(
    "div",
    { class: "nx-head-right sh-head__right" },
    live.el,
    actions,
  );
  const mark = stackMark(spec.name, 40);
  mark.classList.add("sh-head__mark");
  const row = el(
    "div",
    { class: "title-row sh-head__row" },
    mark,
    title,
    state,
    right,
  );
  const desc = /** @type {HTMLParagraphElement} */ (
    el("p", { class: "section-head__desc nx-head-desc" }, spec.desc)
  );
  const meta = el("div", { class: "sh-head__meta", id: "stack-flags" });
  const e = el("header", { class: "nx-head sh-head" }, row, desc, meta);
  return {
    el: e,
    title,
    live,
    /** @param {{label: string, tone: string}} s */
    setState: (s) => state.replaceChildren(dot(s.tone, s.label)),
    /** @param {Parameters<typeof chip>[0][]} list */
    setChips: (list) => meta.replaceChildren(...list.map(chip)),
  };
}

/**
 * @typedef {{label: string, hint: string, danger?: boolean,
 *   attrs?: Record<string, string | null>, onClick?: () => void,
 *   href?: string, download?: string,
 *   mark?: (e: HTMLElement) => void, disabled?: string | null}} MenuEntry
 */

/**
 * A button that opens a grouped menu (the header's More ▾): each entry a
 * label with its one-line hint under it, each group under its small
 * heading. Closed by Escape, a click outside or picking an entry; arrow
 * keys move between entries. A closed menu's entries stay in the DOM, so
 * a Live view step aimed at one marks this menu's button (driveannounce
 * `onScreen`: the first `:scope > button` of the nearest visible parent).
 * @param {{label: string, title: string, groups: {group: string,
 *   items: MenuEntry[]}[], mark?: (b: HTMLElement) => void}} spec
 */
export function groupedMenu(spec) {
  const list = el("div", {
    class: "sh-menu__list",
    role: "menu",
    hidden: true,
    "aria-label": spec.title,
  });
  const btn = el(
    "button",
    {
      type: "button",
      class: "kp-button",
      "aria-haspopup": "menu",
      "aria-expanded": "false",
      title: spec.title,
      "aria-keyshortcuts": ".",
    },
    spec.label,
  );
  spec.mark?.(btn);
  const wrap = el("div", { class: "sh-menu" }, btn, list);
  const open = () => {
    list.hidden = false;
    btn.setAttribute("aria-expanded", "true");
    /** @type {HTMLElement | null} */ (
      list.querySelector("[role=menuitem]:not([disabled])")
    )?.focus();
  };
  const close = (focus = false) => {
    if (list.hidden) return;
    list.hidden = true;
    btn.setAttribute("aria-expanded", "false");
    if (focus) btn.focus();
    if (pending) {
      const p = pending;
      pending = null;
      fill(p);
    }
  };
  btn.addEventListener("click", () => (list.hidden ? open() : close()));
  // review 4: the stack page calls `fill` on every fleet push. Redrawing an
  // open menu threw away the entry that had the keyboard focus, so it is
  // redrawn only when what it shows changed, and never while it is open
  // (the change waits until it closes).
  let drawn = "";
  /** @type {{group: string, items: MenuEntry[]}[] | null} */
  let pending = null;
  /** @param {{group: string, items: MenuEntry[]}[]} groups */
  const fill = (groups) => {
    const sig = menuSignature(groups);
    if (sig === drawn) {
      pending = null;
      return;
    }
    if (!list.hidden) {
      pending = groups;
      return;
    }
    drawn = sig;
    draw(groups);
  };
  /** @param {{group: string, items: MenuEntry[]}[]} groups */
  const draw = (groups) => {
    list.replaceChildren(
      ...groups.flatMap((g) => [
        el("p", { class: "sh-menu__group", role: "presentation" }, g.group),
        ...g.items.map((it) => {
          const item = el(
            it.href ? "a" : "button",
            {
              ...(it.href
                ? { href: it.href, download: it.download ?? null }
                : { type: "button" }),
              role: "menuitem",
              class: `sh-menu__item${it.danger ? " sh-menu__item--danger" : ""}`,
              title: it.disabled ?? it.hint,
              disabled: it.disabled ? true : null,
              ...(it.attrs ?? {}),
            },
            el("b", null, it.label),
            el("span", null, it.disabled ?? it.hint),
          );
          it.mark?.(item);
          item.addEventListener("click", () => {
            close();
            it.onClick?.();
          });
          return item;
        }),
      ]),
    );
  };
  fill(spec.groups);
  list.addEventListener("keydown", (e) => {
    const items = /** @type {HTMLElement[]} */ ([
      ...list.querySelectorAll("[role=menuitem]:not([disabled])"),
    ]);
    const i = items.indexOf(
      /** @type {HTMLElement} */ (document.activeElement),
    );
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const n = items.length;
      if (!n) return;
      const next = e.key === "ArrowDown" ? (i + 1) % n : (i - 1 + n) % n;
      items[next].focus();
    } else if (e.key === "Home") {
      e.preventDefault();
      items[0]?.focus();
    } else if (e.key === "End") {
      e.preventDefault();
      items.at(-1)?.focus();
    }
  });
  /** @param {MouseEvent} e */
  const outside = (e) => {
    if (!wrap.contains(/** @type {Node} */ (e.target))) close();
  };
  /** @param {KeyboardEvent} e */
  const esc = (e) => {
    if (e.key === "Escape" && !list.hidden) {
      e.stopPropagation();
      close(true);
    }
  };
  document.addEventListener("click", outside);
  document.addEventListener("keydown", esc, true);
  return {
    el: wrap,
    button: btn,
    open,
    close,
    toggle: () => (list.hidden ? open() : close()),
    fill,
    stop: () => {
      document.removeEventListener("click", outside);
      document.removeEventListener("keydown", esc, true);
    },
  };
}

/**
 * The side filter column of a log explorer (DESIGN_LANGUAGE §12, the hub
 * demo's Logs): one part per dimension, each value a toggle row with its
 * count, every value on at first. A plain click turns one on or off (no
 * modifier keys, Kenny 2026-10-03); "All" turns a dimension's values back
 * on; `reset()` (Esc) all of them.
 * @param {{label: string, parts: {key: string, label: string,
 *   values: {value: string, label: string}[]}[], hint: string,
 *   onChange: () => void,
 *   mark?: (b: HTMLElement, part: string, value: string) => void}} spec
 */
export function sideFilters(spec) {
  /** @type {Map<string, Set<string>>} values turned OFF, per part */
  const off = new Map(spec.parts.map((p) => [p.key, new Set()]));
  /** @type {Map<string, HTMLElement>} */
  const holders = new Map();
  /** @type {Map<string, {value: string, label: string}[]>} */
  const values = new Map(spec.parts.map((p) => [p.key, p.values]));
  /** @type {Map<string, Map<string, HTMLElement>>} */
  const counts = new Map();
  /** @param {string} key */
  const paint = (key) => {
    const holder = holders.get(key);
    const o = off.get(key) ?? new Set();
    if (!holder) return;
    /** @type {Map<string, HTMLElement>} */
    const cs = new Map();
    holder.replaceChildren(
      ...(values.get(key) ?? []).map((v) => {
        const n = el("small", null);
        cs.set(v.value, n);
        const b = el(
          "button",
          {
            type: "button",
            "aria-pressed": String(!o.has(v.value)),
            "data-value": v.value,
            title: `Show or hide ${v.label}; each click turns it on or off`,
            onclick: () => {
              if (o.has(v.value)) o.delete(v.value);
              else o.add(v.value);
              b.setAttribute("aria-pressed", String(!o.has(v.value)));
              spec.onChange();
            },
          },
          el("span", null, v.label),
          n,
        );
        spec.mark?.(b, key, v.value);
        return b;
      }),
    );
    counts.set(key, cs);
  };
  const parts = spec.parts.map((p) => {
    const holder = el("div", {
      class: "sh-side__opts",
      role: "group",
      "aria-label": `${p.label}: click to show or hide`,
    });
    holders.set(p.key, holder);
    const all = el(
      "button",
      {
        type: "button",
        class: "sh-side__all",
        title: `Show every ${p.label.toLowerCase()} again`,
        onclick: () => {
          off.get(p.key)?.clear();
          paint(p.key);
          spec.onChange();
        },
      },
      "All",
    );
    spec.mark?.(all, p.key, "all");
    paint(p.key);
    return el(
      "div",
      { class: "sh-side__part" },
      el("p", { class: "sh-side__label" }, el("span", null, p.label), all),
      holder,
    );
  });
  const e = el(
    "aside",
    { class: "sh-side__panel", "aria-label": spec.label },
    parts,
    el("p", { class: "sh-hint" }, spec.hint),
  );
  return {
    el: e,
    /** @param {string} key */
    off: (key) => new Set(off.get(key) ?? []),
    /** @param {string} key @param {{value: string, label: string}[]} next */
    setValues: (key, next) => {
      const now = values.get(key) ?? [];
      if (
        now.length === next.length &&
        now.every((v, i) => v.value === next[i].value)
      )
        return;
      values.set(key, next);
      paint(key);
    },
    /**
     * Show only `keep` of a part (a deep link: one app's Logs, the
     * errors): every other value of it turned off.
     * @param {string} key @param {string} keep
     */
    only: (key, keep) => {
      const o = off.get(key);
      if (!o) return;
      const next = (values.get(key) ?? [])
        .map((v) => v.value)
        .filter((v) => v !== keep);
      if (next.length === o.size && next.every((v) => o.has(v))) return;
      o.clear();
      for (const v of next) o.add(v);
      paint(key);
    },
    /** @param {string} key @param {Record<string, number>} n */
    setCounts: (key, n) => {
      for (const [v, c] of counts.get(key) ?? [])
        c.textContent = String(n[v] ?? 0);
    },
    /** @returns {boolean} whether anything was off */
    reset: () => {
      let any = false;
      for (const [k, o] of off) {
        if (o.size) any = true;
        o.clear();
        paint(k);
      }
      return any;
    },
  };
}

/**
 * A card's foot line (DESIGN_LANGUAGE §5): the source on the left, a link
 * or "read 4 s ago" on the right.
 * @param {Child[]} parts
 */
export const foot = (parts) =>
  el(
    "div",
    { class: "sh-foot" },
    parts.map((p) => el("span", null, p)),
  );

/**
 * The 14-night strip of a backup row: one cell per night, oldest first.
 * @param {{night: string, state: string}[]} cells
 */
export function nightStrip(cells) {
  const missed = cells.filter((c) => c.state === "miss").map((c) => c.night);
  const got = cells.filter((c) => c.state === "ok").length;
  return el(
    "span",
    {
      class: "sh-strip",
      role: "img",
      "aria-label": `${got} of the last ${cells.length} nights backed up${missed.length ? `; missed ${missed.join(", ")}` : ""}`,
      title: missed.length
        ? `Missed: ${missed.join(", ")}`
        : `${got} of ${cells.length} nights backed up`,
    },
    cells.map((c) =>
      el("i", {
        class: `sh-strip__${c.state}`,
        title: `${c.night}: ${c.state}`,
      }),
    ),
  );
}
