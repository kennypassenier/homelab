// Live view cursor (Kenny, 2026-09-29, decision live-cursor): a simulated
// "Claude" pointer in a tab that follows. During an announcement's countdown
// it glides from where it was to the element the step acts on (the element
// the highlight marks), arrives shortly before 0, shows a click (a ring and
// a brief pressed look on the target) and the step then runs as before.
// While Claude types it sits in the field. Display only: nothing about what
// is sent, or the once-only press, depends on it.
//
// The pointer lives in a fixed overlay that takes no pointer events and no
// room in the page. A modal dialog sits in the browser's top layer, so the
// overlay is a manual popover that is raised again above each dialog that
// opens after it.
//
// The first half of this file is pure (no DOM, no clock): the path and the
// timing, tested in test/cursor.test.js.

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

/**
 * Should the click show now?
 * @param {number} left countdown left (ms)
 * @param {number} totalMs
 * @param {boolean} paused a paused countdown never clicks
 */
export const clickDue = (left, totalMs, paused) =>
  !paused && totalMs > 0 && left <= clickLead(totalMs);

// ── the pointer on the page ────────────────────────────────────────────

/** How long the glide to a field takes before Claude types (ms). */
const TYPE_GLIDE_MS = 350;

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
   *   clicked: boolean} | {kind: "timed", start: number}} */
  let move = null;
  let frame = 0;
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
        TYPE_GLIDE_MS,
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
      begin();
      target = el;
      move = { kind: "timed", start: performance.now() };
      start();
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
