// redesign-config (3.71.0): small LOCAL building blocks the three
// Configure-side pages share — Firewall, Settings (with Sign-in) and
// Presets — drawn as Kenny's approved demos draw them
// (~/.local/share/homelab/redesign-3.71/{firewall,settings,presets}.html
// and their cfg-aa93.js interaction kit). Shaped like the foundation's
// ui.js API where one exists, so the merge can fold them in:
//
//   pageHeader   ui.js's own, plus a `meta` slot beside the title (the
//                demo's "working copy bb5dce9" / "read 3 s ago" chip)
//   tipOn        one floating detail card for hover AND keyboard focus
//                (cfg-aa93.js `tipOn`); never under the pointer
//   seg          a segmented single choice (the demo's `.nx-seg`)
//   highlight    the search's match marked inside a text
//   rowKeys      j / k move between rows, Enter opens
//   skel         one inline skeleton bar in the final geometry (ui.js
//                has lines, tables and blocks, not one bar in a text)
//   failBox      dom.js's errorBox plus its Try again
//   failBand     one alert for every read a page lost at once
//   dot          status = dot + word (DESIGN_LANGUAGE §4)
//
// Styled by css/pages/configkit.css under the `cf-` prefix.

import { errorBox } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { failSummary } from "../settingsview.js";
import { pageHeader as uiHeader } from "../ui.js";
import { el, ensureStyle } from "./hostkit.js";

export { el, ensureStyle };

/** @typedef {import("./hostkit.js").Child} Child */

/** Load the shared sheet plus the page's own. @param {string} page */
export function pageStyles(page) {
  ensureStyle("/css/pages/configkit.css");
  ensureStyle(`/css/pages/${page}.css`);
}

/**
 * The page header: ui.js's own (the title row with the actions against
 * its right edge, the primary last, the description under it), plus the
 * demo's meta chips right beside the title — the slot ui.js lacks.
 * @param {{title: string, desc: string, meta?: Child[], actions?: Node[],
 *   primary?: Node | null}} spec
 */
export function pageHeader(spec) {
  const head = uiHeader({
    title: spec.title,
    desc: spec.desc,
    actions: spec.actions ?? [],
    primary: spec.primary ?? null,
  });
  const meta = el("div", { class: "cf-head__meta" }, spec.meta ?? []);
  head.title.after(meta);
  return { ...head, meta };
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
 * The text with the query's first match marked (case-insensitive), as ONE
 * element: a chip is a flex box, and loose text nodes beside a `<mark>`
 * would each become a flex item with the chip's gap between them
 * ("c a dvisor", redesign-config-4).
 * @param {string | null | undefined} text
 * @param {string} q lower-case
 * @returns {Child}
 */
export function highlight(text, q) {
  if (!text) return text ?? "";
  if (!q) return text;
  const i = text.toLowerCase().indexOf(q);
  if (i < 0) return text;
  return el(
    "span",
    { class: "cf-hl" },
    text.slice(0, i),
    el("mark", { class: "cf-mark" }, text.slice(i, i + q.length)),
    text.slice(i + q.length),
  );
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
    if (typingTarget(t.tagName)) return;
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
 * A card's error (DESIGN_LANGUAGE §7): dom.js's errorBox (what failed,
 * why, what to do) with its Try again.
 * @param {import("../doctor.js").RouteError} e
 * @param {() => void} retry
 * @param {string} page whose Live view id its Try again carries
 */
export function failBox(e, retry, page) {
  const box = errorBox(e);
  box.classList.add("cf-fail");
  box.append(tryAgain(retry, page));
  return box;
}

// Live view: the one Try again each page's error shows.
const TRY_AGAIN = Object.fromEntries(
  ["firewall", "settings", "presets"].map((page) => [
    page,
    declare({
      id: `${page}-try-again`,
      page,
      opens: "view",
      what: "read what failed again",
    }),
  ]),
);

/** @param {() => void} retry @param {string} page */
const tryAgain = (retry, page) =>
  drivable(
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
    TRY_AGAIN[page] ?? TRY_AGAIN.settings,
  );

/**
 * One alert for every read the page lost at once (redesign-config-11):
 * the cause once, under the header, with ONE Try again; the cards only
 * say they are empty (`failNote`). Hidden while nothing failed.
 * @param {string} page whose Live view id its Try again carries
 */
export function failBand(page) {
  const box = el("div", { class: "cf-failband", hidden: true });
  return {
    el: box,
    /**
     * @param {{what: string, error: import("../doctor.js").RouteError}[]} fails
     * @param {() => void} retry
     */
    set: (fails, retry) => {
      const f = failSummary(fails);
      box.hidden = !f;
      if (!f) return box.replaceChildren();
      box.replaceChildren(
        el(
          "div",
          { class: "kp-alert kp-alert--destructive cf-fail", role: "alert" },
          el("strong", null, f.title),
          el("p", null, `Why: ${f.why}`),
          f.fix ? el("p", null, `What to do: ${f.fix}`) : null,
          tryAgain(retry, page),
        ),
      );
    },
  };
}

/** A card's muted line while the page's one alert says why. @param {string} text */
export const failNote = (text) => el("p", { class: "cf-failnote" }, text);
