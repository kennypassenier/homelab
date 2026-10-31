// refresh-no-replay-1 (Kenny, 2026-10-24: "enkel bij pageload of als een
// component nieuw op het scherm aankomt, NIET bij updates"): a refresh that
// brings new numbers changes the element that shows them, it never builds a
// new one. The theme plays its entrance (a meter's wipe, a row's arrival) on
// every element that is created, so a part that is drawn again from scratch
// replays that entrance, and an opened row closes with it.
//
// `syncNode` makes a live element look like a freshly built one by changing
// only what differs: an attribute, a text, a child that is new or gone. An
// element that stays, stays the same element. `syncRows` does it for the
// keyed rows of a table body, leaving the rows the data table itself added
// (a row's detail, its expand cell) alone. `setBulletsQuiet` does it for the
// kp bullets list, which draws its whole list again on every call.

import { setBullets } from "/static/kp/dist/kp-themes.min.js";
import { setEntity, setEntityGrid } from "/static/kp/dist/kp-themes.min.js";

/** @param {Node} n @returns {string | null} the key an element is matched by */
function keyOf(n) {
  if (!(n instanceof Element)) return null;
  return (
    n.getAttribute("data-kp-bullets-key") ??
    n.getAttribute("data-kp-entity-key") ??
    n.getAttribute("data-kp-row-key") ??
    n.getAttribute("data-label")
  );
}

/**
 * Whether two elements have the same shape: the same tags with the same
 * number of children all the way down (texts and attributes may differ).
 * @param {Node} a
 * @param {Node} b
 * @returns {boolean}
 */
export function sameShape(a, b) {
  if (a.nodeName !== b.nodeName) return false;
  if (a.childNodes.length !== b.childNodes.length) return false;
  for (let i = 0; i < a.childNodes.length; i++)
    if (!sameShape(a.childNodes[i], b.childNodes[i])) return false;
  return true;
}

/**
 * Make `live` look like `next`, changing only the difference. `next` is
 * built by the same code that built `live`; it is thrown away afterwards.
 * @param {Element} live
 * @param {Element} next
 * @param {{prune?: boolean}} [opts] `prune`: attributes `next` lacks are
 *   removed from `live` (right for an element only we or one kp function
 *   draws; wrong for one the data table decorates)
 */
export function syncNode(live, next, opts = {}) {
  for (const a of [...next.attributes])
    if (live.getAttribute(a.name) !== a.value)
      live.setAttribute(a.name, a.value);
  if (opts.prune)
    for (const a of [...live.attributes])
      if (!next.hasAttribute(a.name)) live.removeAttribute(a.name);
  const want = [...next.childNodes];
  /** @type {Map<string, Node>} */
  const keyed = new Map();
  for (const c of live.childNodes) {
    const k = keyOf(c);
    if (k != null) keyed.set(k, c);
  }
  let at = 0;
  for (const n of want) {
    const k = keyOf(n);
    const cur = live.childNodes[at];
    /** @type {Node | undefined} */
    let match =
      k != null
        ? keyed.get(k)
        : cur && keyOf(cur) == null && cur.nodeName === n.nodeName
          ? cur
          : undefined;
    if (match && match.nodeName !== n.nodeName) match = undefined;
    if (!match) {
      live.insertBefore(n, cur ?? null);
      at += 1;
      continue;
    }
    if (match !== cur) live.insertBefore(match, cur ?? null);
    if (match instanceof Element && n instanceof Element)
      syncNode(match, n, opts);
    else if (match.nodeValue !== n.nodeValue) match.nodeValue = n.nodeValue;
    at += 1;
  }
  while (live.childNodes.length > at) live.lastChild?.remove();
}

/**
 * Bring a table body's keyed rows up to date without drawing any row again.
 * A row that is in both stays the same element and only its cells' content
 * changes (cells are matched by `data-label`, so the cells and detail rows
 * the data table added are never touched); a row only in `fresh` is added at
 * the end (the data table's `refresh()` places it in the current sort); a row
 * no longer in `fresh` goes, with its detail row.
 * @param {HTMLElement} tbody
 * @param {HTMLElement[]} fresh rows with `data-kp-row-key`, each fully built
 */
export function syncRows(tbody, fresh) {
  /** @type {Map<string, HTMLElement>} */
  const have = new Map();
  for (const r of /** @type {NodeListOf<HTMLElement>} */ (
    tbody.querySelectorAll(":scope > tr[data-kp-row-key]")
  ))
    have.set(/** @type {string} */ (r.getAttribute("data-kp-row-key")), r);
  const keep = new Set();
  for (const row of fresh) {
    const key = /** @type {string} */ (row.getAttribute("data-kp-row-key"));
    keep.add(key);
    const live = have.get(key);
    if (!live) {
      tbody.append(row);
      continue;
    }
    for (const a of [...row.attributes])
      if (live.getAttribute(a.name) !== a.value)
        live.setAttribute(a.name, a.value);
    for (const cell of /** @type {HTMLElement[]} */ ([...row.children])) {
      const label = cell.getAttribute("data-label");
      const mine = [...live.children].find(
        (c) => c.getAttribute("data-label") === label,
      );
      if (mine) syncNode(mine, cell);
    }
  }
  for (const [key, row] of have) {
    if (keep.has(key)) continue;
    const next = row.nextElementSibling;
    if (next?.hasAttribute("data-kp-row-detail")) next.remove();
    row.remove();
  }
}

/**
 * kp's `setBullets`, but a list that is already drawn and stays a list of
 * rows is changed in place (a bar's width, a figure), so no bar wipes in
 * again and a focused name keeps its focus. Everything else (first draw,
 * loading, empty, failed, back from one of those) is kp's own.
 * @param {HTMLElement} el
 * @param {Parameters<typeof setBullets>[1]} spec
 */
export function setBulletsQuiet(el, spec) {
  const drawn =
    el.getAttribute("data-kp-state") === "ready" &&
    !el.hasAttribute("aria-busy");
  const state = spec.state ?? "ready";
  if (!drawn || state !== "ready" || !(spec.rows?.length ?? 0)) {
    setBullets(el, spec);
    return;
  }
  // kp draws into a detached element synchronously (no busy card there);
  // its list is the picture the live one is made to match.
  const scratch = el.ownerDocument.createElement("div");
  setBullets(scratch, spec);
  const next = scratch.querySelector(":scope > .kp-bullets__list");
  const cur = el.querySelector(":scope > .kp-bullets__list");
  if (!next || !cur) {
    setBullets(el, spec);
    return;
  }
  syncNode(cur, next, { prune: true });
}

/**
 * kp's `setEntityGrid`, but a card that is already there is changed in
 * place: kp draws a card's meters again whenever a figure moved, and each
 * new meter wipes in. The card is drawn once more off to the side (inside
 * the grid, so the grid's options reach it) and the card in view is made to
 * match it. A grid that is not showing cards yet (loading, empty, failed),
 * or is to show none, is kp's own.
 * @param {HTMLElement} grid
 * @param {Parameters<typeof setEntityGrid>[1]} list
 * @param {Parameters<typeof setEntityGrid>[2]} [empty]
 */
export function setEntityGridQuiet(grid, list, empty = {}) {
  const cards = /** @type {HTMLElement[]} */ ([
    ...grid.querySelectorAll(":scope > .kp-card--entity:not(.kp-card--ghost)"),
  ]);
  if (
    grid.hasAttribute("data-kp-state") ||
    cards.length === 0 ||
    list.length === 0
  ) {
    setEntityGrid(grid, list, empty);
    return;
  }
  grid.toggleAttribute(
    "data-kp-entity-trended",
    list.some((d) => Array.isArray(d.trend)),
  );
  /** @type {Map<string, HTMLElement>} */
  const have = new Map();
  for (const card of cards) {
    const k = card.getAttribute("data-kp-entity-key");
    if (k && list.some((d) => d.key === k)) have.set(k, card);
    else card.remove();
  }
  let at = 0;
  for (const data of list) {
    let card = have.get(data.key);
    if (!card) {
      card = grid.ownerDocument.createElement("article");
      card.setAttribute("role", "listitem");
      grid.insertBefore(card, grid.children[at] ?? null);
      setEntity(card, data);
    } else {
      if (grid.children[at] !== card)
        grid.insertBefore(card, grid.children[at] ?? null);
      const side = grid.ownerDocument.createElement("article");
      side.setAttribute("role", "listitem");
      side.hidden = true;
      grid.append(side);
      setEntity(side, data);
      side.hidden = false;
      // Same shape: only texts and attributes move, so every element keeps
      // the listeners the page and kp gave it. Another shape: kp redraws it.
      if (sameShape(card, side)) {
        syncNode(card, side);
        side.remove();
      } else {
        side.remove();
        setEntity(card, data);
      }
    }
    at += 1;
  }
}
