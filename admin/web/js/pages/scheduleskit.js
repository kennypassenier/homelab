// redesign-schedules (release 3.71.0): small local stand-ins for the shared
// components the 3.71 foundation is building in ui.js, named the way ui.js
// will name them (pageHeader, section, toast), so the Schedules page can
// swap them for the shared ones without changing its own code. Kept here
// rather than in ui.js on purpose: the foundation is being built in
// parallel and this page must not depend on it yet.
//
// Loading this module also loads the page's own stylesheet once.

const SHEET = "/css/schedules.css";
if (
  typeof document !== "undefined" &&
  !document.querySelector(`link[href="${SHEET}"]`)
) {
  const link = document.createElement("link");
  link.rel = "stylesheet";
  link.href = SHEET;
  document.head.append(link);
}

/**
 * @typedef {Node | string | null | undefined | false} Child
 * @typedef {Child | Child[]} Children
 * @typedef {Record<string, string | number | boolean | null | undefined |
 *   ((e: any) => void)>} Attrs
 */

/**
 * Build an element: `on…` keys add listeners, `false`/`null` attributes
 * are left out, arrays of children are flattened.
 * @template {keyof HTMLElementTagNameMap} K
 * @param {K} tag
 * @param {Attrs | null} [attrs]
 * @param {...Children} children
 * @returns {HTMLElementTagNameMap[K]}
 */
export function el(tag, attrs, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs ?? {})) {
    if (v == null || v === false) continue;
    if (typeof v === "function") e.addEventListener(k.slice(2), v);
    else if (k === "class") e.className = String(v);
    else e.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children.flat()) if (c != null && c !== false) e.append(c);
  return e;
}

/**
 * A page's header: title, one-sentence description, a meta line and the
 * page's actions on the right (folded under the title on a phone).
 * @param {{title: string, desc: string, meta?: Children,
 *   actions?: Children}} spec
 */
export function pageHeader(spec) {
  return el(
    "header",
    { class: "sch-ph" },
    el("h1", null, spec.title),
    el("p", { class: "sch-ph__desc" }, spec.desc),
    el("div", { class: "sch-ph__meta" }, spec.meta ?? null),
    el("div", { class: "sch-ph__actions" }, spec.actions ?? null),
  );
}

/**
 * One top-level section: a card with a heading and its one-sentence
 * description (rule 8), a body and an optional foot line.
 * @param {{title?: string, desc?: string, body: Children, foot?: string,
 *   id?: string, label?: string}} spec
 */
export function section(spec) {
  return el(
    "section",
    {
      class: "sch-card",
      id: spec.id,
      "aria-label": spec.title ? null : (spec.label ?? null),
    },
    spec.title
      ? el(
          "div",
          { class: "sch-card__h" },
          el("h2", null, spec.title),
          el("p", null, spec.desc ?? ""),
        )
      : el("span"),
    el("div", { class: "sch-card__b" }, spec.body),
    spec.foot ? el("div", { class: "sch-card__f" }, spec.foot) : null,
  );
}

/** A keyboard key. @param {string} k */
export const kbd = (k) => el("span", { class: "sch-kbd" }, k);

/**
 * The line of shortcuts at the foot of a page.
 * @param {[string, string][]} pairs key, what it does
 */
export const keysLine = (pairs) =>
  el(
    "p",
    { class: "sch-keys" },
    pairs.map(([k, what]) => el("span", null, kbd(k), what)),
  );

/** How long a toast with Undo stays, in milliseconds. */
export const UNDO_MS = 6000;

/** @type {{el: HTMLElement, timer: ReturnType<typeof setTimeout>} | null} */
let shown = null;

/**
 * One toast at a time at the foot of the screen, gone after six seconds;
 * with `undo`, an Undo button that calls it and closes the toast.
 * @param {string} text
 * @param {() => void} [undo]
 * @param {{host?: HTMLElement, mark?: (button: HTMLElement) => void}} [opts]
 *   host: where it is drawn (a page's own root, so Live view finds its Undo
 *   among the page's controls); mark: declares the Undo button for it
 * @returns {() => void} close it now
 */
export function toast(text, undo, opts = {}) {
  if (shown) {
    clearTimeout(shown.timer);
    shown.el.remove();
  }
  const box = el(
    "div",
    { class: "sch-toast", role: "status" },
    el("span", null, text),
    undo
      ? el(
          "button",
          {
            type: "button",
            onclick: () => {
              close();
              undo();
            },
          },
          "Undo",
        )
      : null,
  );
  const button = box.querySelector("button");
  if (button && opts.mark) opts.mark(button);
  const close = () => {
    if (shown?.el === box) {
      clearTimeout(shown.timer);
      shown = null;
    }
    box.remove();
  };
  (opts.host ?? document.body).append(box);
  shown = { el: box, timer: setTimeout(close, UNDO_MS) };
  return close;
}
