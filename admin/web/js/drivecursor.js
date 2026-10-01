// Live view cursor (Kenny, 2026-09-29, decision live-cursor): a simulated
// "Claude" pointer in a tab that follows. During an announcement's countdown
// it glides from where it was to the element the step acts on (the element
// the highlight marks), arrives shortly before 0, shows a click (a ring and
// a brief pressed look on the target) and the step then runs as before.
// While Claude types it sits in the field; a dropdown pick draws the list
// and passes over its options to the choice. A target outside the safe
// band (below the top bar, above the announce bar) is scrolled into it
// first, in the page or in the dialog's own scrolling body. Display only: nothing about what
// is sent, or the once-only press, depends on it.
//
// The pointer lives in a fixed overlay that takes no pointer events and no
// room in the page. A modal dialog sits in the browser's top layer, so the
// overlay is a manual popover that is raised again above each dialog that
// opens after it.
//
// The first half of this file is pure (no DOM, no clock): the path and the
// timing, tested in test/cursor.test.js. The pace of typing and picking
// is in drivepace.js.

import {
  PICK_CLOSE_MS,
  REDUCED_PICK_MS,
  TYPE_GLIDE_MS,
  listTop,
  pickPlan,
} from "./drivepace.js";

/**
 * @typedef {{x: number, y: number}} Point
 * @typedef {{left: number, top: number, width: number, height: number}} Box
 */

/** Keep the pointer this far inside the window's edges (px). */
export const EDGE = 12;

/**
 * How long before 0 the glide arrives: a little rest on the target before
 * the click, scaled to the countdown and never more than 600 ms.
 * @param {number} totalMs the announcement's whole countdown
 */
export const arriveLead = (totalMs) =>
  Math.max(0, Math.min(600, Math.round(totalMs * 0.12)));

/**
 * When the click shows: this long before 0, so the ring is seen as the
 * step lands.
 * @param {number} totalMs
 */
export const clickLead = (totalMs) =>
  Math.max(0, Math.min(180, Math.round(totalMs * 0.04)));

/**
 * How far along the glide is, 0..1. The glide starts when the cursor saw
 * the announcement (`startLeft` left then) and ends `arriveLead` before 0,
 * so a tab that joins late still glides, only faster. A pause freezes
 * `left`, so it freezes the glide too.
 * @param {number} startLeft countdown left when the glide began (ms)
 * @param {number} left countdown left now (ms)
 * @param {number} totalMs
 * @param {boolean} [reduced] prefers-reduced-motion: jump at once
 */
export function glideFraction(startLeft, left, totalMs, reduced = false) {
  if (reduced) return 1;
  const span = startLeft - arriveLead(totalMs);
  if (span <= 0) return 1;
  return Math.max(0, Math.min(1, (startLeft - left) / span));
}

/**
 * How far along a glide measured in time is (typing: no countdown).
 * @param {number} elapsed ms since the glide began
 * @param {number} duration ms
 * @param {boolean} [reduced]
 */
export const timedFraction = (elapsed, duration, reduced = false) =>
  reduced || duration <= 0 ? 1 : Math.max(0, Math.min(1, elapsed / duration));

/**
 * Ease in and out (cubic): slow off, quick across, gentle landing.
 * @param {number} t 0..1
 */
export const ease = (t) =>
  t < 0.5 ? 4 * t * t * t : 1 - Math.pow(-2 * t + 2, 3) / 2;

/**
 * The point between `a` and `b` at eased fraction `t`.
 * @param {Point} a
 * @param {Point} b
 * @param {number} t 0..1, not yet eased
 * @returns {Point}
 */
export function pathAt(a, b, t) {
  const e = ease(Math.max(0, Math.min(1, t)));
  return { x: a.x + (b.x - a.x) * e, y: a.y + (b.y - a.y) * e };
}

/**
 * Where the pointer's tip goes on a target: its centre, kept inside the
 * window so a target scrolled away still has the pointer at the nearest
 * edge.
 * @param {Box} box the target's client rectangle
 * @param {number} vw window width
 * @param {number} vh window height
 * @returns {Point}
 */
export function aimAt(box, vw, vh) {
  const clamp = (/** @type {number} */ v, /** @type {number} */ max) =>
    Math.max(EDGE, Math.min(Math.max(EDGE, max - EDGE), v));
  return {
    x: clamp(box.left + box.width / 2, vw),
    y: clamp(box.top + box.height / 2, vh),
  };
}

/** Keep a target at least this far from the band's edges (px). */
export const BAND_MARGIN = 24;

/**
 * @typedef {{top: number, bottom: number}} Band
 */

/**
 * The safe band (Kenny, 2026-09-29: the pointer ended at the bottom, under
 * the bar that says what comes next): the part of the window below the
 * top chrome and above the announce bar, less a margin. A window too short
 * for a band gets the whole window.
 * @param {number} vh window height
 * @param {number} topChrome bottom edge of a fixed or sticky top bar (px)
 * @param {number} bottomBar height the announce bar covers from below (px)
 * @param {number} [margin]
 * @returns {Band}
 */
export function safeBand(vh, topChrome, bottomBar, margin = BAND_MARGIN) {
  const top = Math.max(0, topChrome) + margin;
  const bottom = vh - Math.max(0, bottomBar) - margin;
  return bottom - top < 80 ? { top: 0, bottom: vh } : { top, bottom };
}

/**
 * The band inside a scroll container that shows only part of it (the
 * action dialog's body): the overlap of the two, less a smaller margin.
 * @param {Band} band the window's band
 * @param {number} top the container's visible top (client px)
 * @param {number} bottom the container's visible bottom (client px)
 * @param {number} [margin]
 * @returns {Band}
 */
export function clipBand(band, top, bottom, margin = BAND_MARGIN / 2) {
  const t = Math.max(band.top, top + margin);
  const b = Math.min(band.bottom, bottom - margin);
  return b - t < 40 ? { top, bottom } : { top: t, bottom: b };
}

/**
 * Is the target inside the band? A target taller than half the band
 * counts once its top and that much of it are inside.
 * @param {Box} box the target's client rectangle
 * @param {Band} band
 */
export function inBand(box, band) {
  const h = Math.min(box.height, (band.bottom - band.top) / 2);
  return box.top >= band.top && box.top + h <= band.bottom;
}

/**
 * How far to scroll (px, positive = down) so the target sits comfortably
 * in the band: its centre on the band's middle, or a tall target's top a
 * sixth of the way down. 0 when it is inside the band already.
 * @param {Box} box
 * @param {Band} band
 */
export function bandScroll(box, band) {
  if (inBand(box, band)) return 0;
  const bh = band.bottom - band.top;
  if (box.height > bh / 2) return Math.round(box.top - (band.top + bh / 6));
  return Math.round(box.top + box.height / 2 - (band.top + bh / 2));
}

/**
 * Should the click show now?
 * @param {number} left countdown left (ms)
 * @param {number} totalMs
 * @param {boolean} paused a paused countdown never clicks
 */
export const clickDue = (left, totalMs, paused) =>
  !paused && totalMs > 0 && left <= clickLead(totalMs);

// ── the pointer on the page ────────────────────────────────────────────

/** @returns {boolean} */
const reducedMotion = () =>
  !!globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

/**
 * What the pointer aims at inside a target: a field's own input, not the
 * middle between its label and its hint.
 * @param {HTMLElement} el
 * @returns {HTMLElement}
 */
function aimElement(el) {
  if (!el.classList.contains("kp-field")) return el;
  const inner = /** @type {HTMLElement | null} */ (
    [...el.querySelectorAll("input, select, textarea, [role=switch]")].find(
      (x) =>
        x.getClientRects().length > 0 &&
        /** @type {HTMLInputElement} */ (x).type !== "hidden",
    ) ?? null
  );
  return inner ?? el;
}

/**
 * @typedef {{
 *   show: (on: boolean) => void,
 *   glide: (el: HTMLElement | null, id: number, total: number,
 *     left: () => number, paused: () => boolean) => void,
 *   sit: (el: HTMLElement | null) => void,
 *   pick: (el: HTMLSelectElement, value: string,
 *     commit: () => void) => Promise<void>,
 *   stepped: () => void,
 * }} Cursor
 */

/**
 * The Claude cursor: one per tab, drawn only while shown.
 * @returns {Cursor}
 */
export function makeCursor() {
  const svgNs = "http://www.w3.org/2000/svg";
  const layer = document.createElement("div");
  layer.className = "drive-cursor";
  layer.setAttribute("aria-hidden", "true");
  const canPop = typeof layer.showPopover === "function";
  if (canPop) layer.setAttribute("popover", "manual");
  const pointer = document.createElement("div");
  pointer.className = "drive-cursor__pointer";
  const svg = document.createElementNS(svgNs, "svg");
  svg.setAttribute("viewBox", "0 0 20 24");
  svg.setAttribute("class", "drive-cursor__arrow");
  const path = document.createElementNS(svgNs, "path");
  // The tip is at (1, 1): the pointer's own origin.
  path.setAttribute(
    "d",
    "M1 1 L1 19 L5.8 14.6 L9 22 L12.4 20.6 L9.3 13.4 L15.8 13.4 Z",
  );
  svg.append(path);
  const label = document.createElement("span");
  label.className = "drive-cursor__label";
  label.textContent = "Claude";
  pointer.append(svg, label);
  const ring = document.createElement("div");
  ring.className = "drive-cursor__ring";
  layer.append(ring, pointer);
  document.body.append(layer);

  let shown = false;
  /** @type {Point | null} */
  let pos = null;
  /** @type {Point} */
  let from = { x: 0, y: 0 };
  /** @type {HTMLElement | null} */
  let target = null;
  /** @type {null | {kind: "count", id: number, total: number,
   *   startLeft: number, left: () => number, paused: () => boolean,
   *   clicked: boolean} | {kind: "timed", start: number, dur: number}} */
  let move = null;
  let frame = 0;
  /** When `tick` last re-revealed a drifting target (ms, performance.now). */
  let revealedAt = 0;
  /** @type {Element | null} */
  let raisedOver = null;
  /** @type {ReturnType<typeof setTimeout> | undefined} */
  let hideTimer;

  const place = (/** @type {Point} */ p) => {
    pos = p;
    pointer.style.transform = `translate(${p.x.toFixed(1)}px, ${p.y.toFixed(1)}px)`;
  };

  /** Above a modal dialog opened after the overlay (both top layer). */
  const raise = () => {
    const d = [...document.querySelectorAll("dialog[open]")].pop() ?? null;
    if (d === raisedOver || !canPop) return;
    raisedOver = d;
    try {
      layer.hidePopover();
      layer.showPopover();
    } catch {
      // Not connected or not supported: stays under the dialog.
    }
  };

  /** The target's aim point now; null once it left the page. */
  const aimNow = () => {
    if (!target?.isConnected || target.getClientRects().length === 0)
      return null;
    const r = aimElement(target).getBoundingClientRect();
    return aimAt(r, innerWidth, innerHeight);
  };

  const click = () => {
    if (!pos) return;
    const reduced = reducedMotion();
    // One transform: the ring's own scale must not scale its offset.
    const at = `translate(${pos.x.toFixed(1)}px, ${pos.y.toFixed(1)}px)`;
    ring.style.transform = at;
    ring.getAnimations().forEach((a) => a.cancel());
    ring.animate(
      reduced
        ? [{ opacity: 0.9 }, { opacity: 0.9, offset: 0.7 }, { opacity: 0 }]
        : [
            { opacity: 0.9, transform: `${at} scale(0.3)` },
            { opacity: 0, transform: `${at} scale(1.8)` },
          ],
      { duration: reduced ? 450 : 520, easing: "ease-out" },
    );
    pointer.classList.add("drive-cursor__pointer--down");
    const t = target;
    t?.classList.add("drive-press");
    setTimeout(() => {
      pointer.classList.remove("drive-cursor__pointer--down");
      t?.classList.remove("drive-press");
    }, 200);
  };

  const tick = () => {
    frame = 0;
    if (!shown) return;
    raise();
    // Kenny, 2026-10-01: the page under a step can still be rendering
    // when the step is announced (a stack page fills in after `goto`), so
    // the target revealed at the start drifts below the window and the
    // pointer ends up "over nothing". While the pointer is heading for a
    // target, keep it revealed: re-check at most every 300 ms.
    if (move && target?.isConnected) {
      const now = performance.now();
      if (now - revealedAt > 300) {
        revealedAt = now;
        reveal(target);
      }
    }
    const aim = aimNow();
    if (move?.kind === "count") {
      const left = move.left();
      const t = glideFraction(
        move.startLeft,
        left,
        move.total,
        reducedMotion(),
      );
      if (aim) place(pathAt(from, aim, t));
      if (!move.clicked && aim && clickDue(left, move.total, move.paused())) {
        move.clicked = true;
        click();
      }
    } else if (move?.kind === "timed") {
      const t = timedFraction(
        performance.now() - move.start,
        move.dur,
        reducedMotion(),
      );
      if (aim) place(pathAt(from, aim, t));
    } else if (aim) {
      // Resting on its target: it follows a scroll or a resize.
      place(aim);
    }
    frame = requestAnimationFrame(tick);
  };

  const start = () => {
    if (!frame && shown) frame = requestAnimationFrame(tick);
  };

  /** The window's safe band now: under a fixed or sticky top bar, above
   * the announce bar while it shows (both measured, never assumed). */
  const bandNow = () => {
    let top = 0;
    for (const bar of document.querySelectorAll(".kp-nav-wrap, .kp-nav")) {
      const pos = getComputedStyle(bar).position;
      if (pos !== "fixed" && pos !== "sticky") continue;
      const r = bar.getBoundingClientRect();
      if (r.height > 0 && r.top <= 1 && r.bottom > top) top = r.bottom;
    }
    let below = 0;
    for (const bar of document.querySelectorAll(
      ".drive-announce:not(.drive-announce--inline)",
    )) {
      if (bar.getClientRects().length === 0) continue;
      const r = bar.getBoundingClientRect();
      if (r.height > 0) below = Math.max(below, innerHeight - r.top);
    }
    return safeBand(innerHeight, top, below);
  };

  /** The element that scrolls `el` into view: its nearest scrolling
   * ancestor (the dialog's body), else the page. */
  const scroller = (/** @type {Element} */ el) => {
    for (let p = el.parentElement; p; p = p.parentElement) {
      if (p === document.body || p === document.documentElement) break;
      const oy = getComputedStyle(p).overflowY;
      if (
        (oy === "auto" || oy === "scroll" || oy === "overlay") &&
        p.scrollHeight > p.clientHeight + 1
      )
        return p;
    }
    return null;
  };

  /** Keep the target inside the safe band: scroll the page, or the
   * dialog's body, so the pointer is seen reaching it and the announce bar
   * never covers what it acts on. True when it scrolled. */
  const reveal = (/** @type {HTMLElement} */ el) => {
    if (!el.isConnected) return false;
    const t = aimElement(el);
    const box = t.getBoundingClientRect();
    const behavior = reducedMotion() ? "auto" : "smooth";
    let band = bandNow();
    const box0 = scroller(t);
    if (box0) {
      const r = box0.getBoundingClientRect();
      const top = r.top + box0.clientTop;
      band = clipBand(band, top, top + box0.clientHeight);
      const dy = bandScroll(box, band);
      if (dy) box0.scrollBy({ top: dy, behavior });
      return dy !== 0;
    }
    const dy = bandScroll(box, band);
    if (dy) scrollBy({ top: dy, behavior });
    return dy !== 0;
  };

  const wait = (/** @type {number} */ ms) =>
    new Promise((r) => setTimeout(r, Math.max(0, ms)));

  /** Glide to `el` over `ms`, then rest there. */
  const glideTo = async (
    /** @type {HTMLElement} */ el,
    /** @type {number} */ ms,
  ) => {
    begin();
    target = el;
    move = { kind: "timed", start: performance.now(), dur: ms };
    start();
    await wait(ms);
  };

  /** The drawn list of a dropdown's options (a native list cannot be
   * opened by a script): under the dropdown, the pointer above it. */
  const list = document.createElement("div");
  list.className = "drive-cursor__list";
  list.hidden = true;
  layer.insertBefore(list, pointer);

  const closeList = () => {
    list.hidden = true;
    list.replaceChildren();
  };

  /** @param {HTMLSelectElement} sel @returns {HTMLElement[]} */
  const openList = (sel) => {
    const r = sel.getBoundingClientRect();
    const rows = [...sel.options].map((o, i) => {
      const row = document.createElement("div");
      row.className = "drive-cursor__option";
      if (i === sel.selectedIndex)
        row.classList.add("drive-cursor__option--on");
      if (o.disabled) row.classList.add("drive-cursor__option--off");
      row.textContent = o.label || o.text || o.value || "\u00a0";
      return row;
    });
    list.replaceChildren(...rows);
    list.style.minWidth = `${Math.round(r.width)}px`;
    list.style.transform = "translate(0px, 0px)";
    list.hidden = false;
    const y = listTop(r, list.offsetHeight, bandNow());
    list.style.transform = `translate(${r.left.toFixed(1)}px, ${y.toFixed(1)}px)`;
    return rows;
  };

  const hover = (/** @type {HTMLElement[]} */ rows, /** @type {number} */ i) =>
    rows.forEach((row, k) =>
      row.classList.toggle("drive-cursor__option--hover", k === i),
    );

  const begin = () => {
    from = pos ?? { x: innerWidth / 2, y: innerHeight / 2 };
    if (!pos) place(from);
  };

  return {
    show(on) {
      if (on === shown) return;
      shown = on;
      clearTimeout(hideTimer);
      if (on) {
        if (canPop && !layer.matches(":popover-open")) {
          try {
            layer.showPopover();
          } catch {
            // Falls back to the page's own stacking.
          }
          raisedOver = null;
        }
        if (!pos) place({ x: innerWidth / 2, y: innerHeight / 2 });
        requestAnimationFrame(() => layer.classList.add("drive-cursor--on"));
        start();
        return;
      }
      layer.classList.remove("drive-cursor--on");
      closeList();
      move = null;
      target = null;
      pos = null;
      hideTimer = setTimeout(() => {
        if (shown || !canPop) return;
        try {
          layer.hidePopover();
        } catch {
          // Already hidden.
        }
      }, 320);
    },
    glide(el, id, total, left, paused) {
      if (move?.kind === "count" && move.id === id && target === el) return;
      if (el) reveal(el);
      begin();
      target = el;
      move = el
        ? {
            kind: "count",
            id,
            total,
            startLeft: left(),
            left,
            paused,
            clicked: false,
          }
        : null;
      start();
    },
    sit(el) {
      if (!el) return;
      reveal(el);
      begin();
      target = el;
      move = { kind: "timed", start: performance.now(), dur: TYPE_GLIDE_MS };
      start();
    },
    async pick(el, value, commit) {
      const plan = pickPlan(
        el.selectedIndex,
        [...el.options].findIndex((o) => o.value === value),
        Math.random,
        reducedMotion(),
      );
      if (!shown || !plan || !el.isConnected) {
        if (shown && el.isConnected) {
          reveal(el);
          begin();
          target = el;
          move = null;
          start();
        }
        commit();
        el.closest(".kp-field")?.classList.add("drive-focus");
        await wait(REDUCED_PICK_MS);
        el.closest(".kp-field")?.classList.remove("drive-focus");
        return;
      }
      const wrap = el.closest(".kp-field");
      wrap?.classList.add("drive-focus");
      try {
        const scrolled = reveal(el);
        // The announcement's glide usually brought the pointer here and
        // clicked already; otherwise glide and click now.
        const r = el.getBoundingClientRect();
        const aim = aimAt(r, innerWidth, innerHeight);
        const there =
          !scrolled &&
          !!pos &&
          !!target &&
          aimElement(target) === el &&
          Math.hypot(pos.x - aim.x, pos.y - aim.y) < 6;
        if (!there) {
          await glideTo(el, plan.glideMs);
          if (!shown) return commit();
          click();
        }
        const rows = openList(el);
        await wait(plan.openPauseMs);
        for (const hv of plan.hovers) {
          if (!shown || !el.isConnected) break;
          const row = rows[hv.index];
          if (!row) continue;
          // A long list shows part of itself: bring the row into it.
          if (row.offsetTop < list.scrollTop) list.scrollTop = row.offsetTop;
          else if (
            row.offsetTop + row.offsetHeight >
            list.scrollTop + list.clientHeight
          )
            list.scrollTop =
              row.offsetTop + row.offsetHeight - list.clientHeight;
          await glideTo(row, hv.moveMs);
          hover(rows, hv.index);
          await wait(hv.restMs);
        }
        if (shown) await wait(plan.choicePauseMs);
        if (shown) click();
        commit();
        rows.forEach((row, k) =>
          row.classList.toggle(
            "drive-cursor__option--on",
            k === el.selectedIndex,
          ),
        );
        await wait(PICK_CLOSE_MS);
      } finally {
        closeList();
        wrap?.classList.remove("drive-focus");
        // Back to rest on the dropdown itself.
        if (shown && el.isConnected) {
          target = el;
          move = null;
        }
      }
    },
    stepped() {
      // The step came before this tab's clock reached the click (a late
      // tab, or a held step without a countdown): click now.
      if (move?.kind === "count" && !move.clicked && aimNow()) {
        move.clicked = true;
        click();
      }
      if (move?.kind === "count") move = null;
    },
  };
}
